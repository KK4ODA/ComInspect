//! Application state and the glue between the monitor, the inventory and
//! the UI.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use tauri::{AppHandle, Emitter, Manager};

use cominspect_core::model::ScanResult;
use cominspect_platform::monitor::{Monitor, MonitorConfig, ScanTrigger};
use cominspect_store::view::{InventoryEvent, InventoryView, PortStatus};
use cominspect_store::{Inventory, OpenReport, Store};

use crate::error::CmdResult;
use crate::lifecycle::{self, StartupInfo};
use crate::updates::UpdateState;
use crate::usage::{self, UsageState};

pub const EVENT_INVENTORY_UPDATED: &str = "inventory://updated";
pub const EVENT_INVENTORY_EVENTS: &str = "inventory://events";

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub log_dir: PathBuf,
}

pub struct AppState {
    pub inventory: Mutex<Inventory>,
    pub monitor: Mutex<Option<Monitor>>,
    pub paths: AppPaths,
    pub open_report: Mutex<OpenReport>,
    pub startup: StartupInfo,
    pub updates: UpdateState,
    pub usage: UsageState,
    pub version: String,
}

/// Locks a mutex, recovering the data if another thread panicked while
/// holding it (the inventory is always left in a consistent state because
/// every database change is transactional).
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl AppState {
    pub fn initialize(app: &AppHandle) -> Result<AppState, Box<dyn std::error::Error>> {
        let data_dir = app.path().app_local_data_dir()?;
        let log_dir = app.path().app_log_dir()?;
        let db_path = data_dir.join("inventory.db");
        let version = app.package_info().version.to_string();

        let (store, report) = match Store::open(&db_path, &version) {
            Ok(opened) => opened,
            Err(e) => {
                log::error!("cannot open device database {}: {e}", db_path.display());
                let reason = format!(
                    "The device database could not be opened ({e}). ComInspect is running with a \
                     temporary database; changes made now will not be saved."
                );
                let store = Store::open_temporary(&version, reason.clone())?;
                let report = OpenReport {
                    path: db_path.clone(),
                    temporary_reason: Some(reason),
                    ..Default::default()
                };
                (store, report)
            }
        };
        if !report.migrations_applied.is_empty() {
            log::info!(
                "database migrated from schema {} to {} (backup: {:?})",
                report.schema_before,
                report.schema_after,
                report.backup
            );
        }
        let startup = lifecycle::record_launch(&store, &version).unwrap_or_else(|e| {
            log::warn!("could not record launch: {e}");
            StartupInfo {
                version: version.clone(),
                ..Default::default()
            }
        });
        let updates = UpdateState::load(app, &store, &version);
        let inventory = Inventory::new(store, cominspect_platform::platform_name());

        log::info!(
            "ComInspect {version} starting on {} ({}); database {}",
            cominspect_platform::platform_name(),
            std::env::consts::ARCH,
            db_path.display()
        );
        Ok(AppState {
            inventory: Mutex::new(inventory),
            monitor: Mutex::new(None),
            paths: AppPaths {
                data_dir,
                db_path,
                log_dir,
            },
            open_report: Mutex::new(report),
            startup,
            updates,
            usage: UsageState::default(),
            version,
        })
    }
}

/// Reconciles a scan and notifies the UI.
pub fn apply_scan(
    app: &AppHandle,
    scan: ScanResult,
    trigger: ScanTrigger,
) -> CmdResult<InventoryView> {
    let state = app.state::<AppState>();
    let (view, events) = {
        let mut inventory = lock(&state.inventory);
        let outcome = inventory.reconcile(scan)?;
        (inventory.view()?, outcome.events)
    };
    if let Err(e) = app.emit(EVENT_INVENTORY_UPDATED, &view) {
        log::warn!("could not emit inventory update: {e}");
    }
    // Keep the usage check on the ports that are connected now.
    let connected = view
        .rows
        .iter()
        .filter(|r| matches!(r.status, PortStatus::Connected | PortStatus::Problem))
        .filter_map(|r| {
            let port = r.port.clone()?;
            let label = r.nickname.clone().unwrap_or_else(|| r.device_label.clone());
            Some((port, label))
        })
        .collect();
    state.usage.set_ports(app, connected);
    let live: Vec<InventoryEvent> = events.into_iter().filter(|e| !e.initial).collect();
    for e in &live {
        log::info!(
            "{} {} {}{}",
            e.kind.as_str(),
            e.label,
            e.port.as_deref().unwrap_or(""),
            e.previous_port
                .as_deref()
                .map(|p| format!(" (was {p})"))
                .unwrap_or_default()
        );
    }
    if !live.is_empty() {
        let _ = app.emit(EVENT_INVENTORY_EVENTS, &live);
    }
    log::debug!("scan applied ({trigger:?})");
    Ok(view)
}

pub fn start_monitor(app: &AppHandle) {
    let handle = app.clone();
    let monitor = Monitor::start(
        MonitorConfig::for_current_platform(),
        move |scan, trigger| {
            if let Err(e) = apply_scan(&handle, scan, trigger) {
                log::error!("could not apply scan: {e}");
            }
        },
    );
    *lock(&app.state::<AppState>().monitor) = Some(monitor);
    usage::start(app);
}

pub fn stop_monitor(app: &AppHandle) {
    usage::stop(app);
    if let Some(state) = app.try_state::<AppState>() {
        let monitor = lock(&state.monitor).take();
        drop(monitor);
    }
}

/// Host name for export files (used to tell whether an import comes from
/// the same computer).
pub fn hostname() -> Option<String> {
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME").ok().filter(|h| !h.is_empty())
    }
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: buffer and length are valid.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
        if rc != 0 {
            return None;
        }
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        let name = String::from_utf8_lossy(&buf[..end]).trim().to_string();
        (!name.is_empty()).then_some(name)
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}
