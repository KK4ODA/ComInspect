//! Commands invoked by the UI. The UI has no file-system or shell access of
//! its own: file dialogs, file I/O and URL opening all happen here.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use cominspect_core::export::{DEFAULT_EXPORT_FILE_NAME, ExportDocument, ImportMode};
use cominspect_core::user::IdentityPatch;
use cominspect_platform::cat::{self, CatProbeInfo, CatProtocol};
use cominspect_platform::diagnostics::{
    self, CatQueryReport, ControlLine, OpenTestReport, PttTestReport, SerialSettings,
};
use cominspect_platform::monitor::ScanTrigger;
use cominspect_store::view::{DeviceDetail, ImportReport, InventoryView, StorageInfo};
use cominspect_store::{BackupInfo, Inventory, Store, list_backups};

use crate::error::{CmdResult, CommandError};
use crate::lifecycle::{self, StartupInfo};
use crate::state::{AppState, apply_scan, hostname, lock};
use crate::updates::{self, DownloadEvent, UpdateChannel, UpdateStatus};
use crate::usage::UsageView;
use crate::vspe::{self, VspeSource, VspeView};

// --- inventory ----------------------------------------------------------------

#[tauri::command]
pub async fn get_inventory(state: State<'_, AppState>) -> CmdResult<InventoryView> {
    Ok(lock(&state.inventory).view()?)
}

#[tauri::command]
pub async fn refresh(app: AppHandle) -> CmdResult<InventoryView> {
    let scan = tauri::async_runtime::spawn_blocking(cominspect_platform::discover)
        .await
        .map_err(|e| CommandError::msg(e.to_string()))?;
    apply_scan(&app, scan, ScanTrigger::Manual)
}

#[tauri::command]
pub async fn get_device_detail(
    state: State<'_, AppState>,
    device_id: i64,
) -> CmdResult<DeviceDetail> {
    Ok(lock(&state.inventory).detail(device_id)?)
}

#[tauri::command]
pub async fn update_identity(
    state: State<'_, AppState>,
    device_id: i64,
    patch: IdentityPatch,
) -> CmdResult<InventoryView> {
    let mut inventory = lock(&state.inventory);
    inventory.update_identity(device_id, &patch)?;
    Ok(inventory.view()?)
}

#[tauri::command]
pub async fn set_ignored(
    state: State<'_, AppState>,
    device_id: i64,
    ignored: bool,
) -> CmdResult<InventoryView> {
    let mut inventory = lock(&state.inventory);
    inventory.set_ignored(device_id, ignored)?;
    Ok(inventory.view()?)
}

#[tauri::command]
pub async fn merge_devices(
    state: State<'_, AppState>,
    target_id: i64,
    source_id: i64,
) -> CmdResult<InventoryView> {
    let mut inventory = lock(&state.inventory);
    inventory.merge(target_id, source_id)?;
    Ok(inventory.view()?)
}

#[tauri::command]
pub async fn forget_device(app: AppHandle, device_id: i64) -> CmdResult<InventoryView> {
    {
        let state = app.state::<AppState>();
        lock(&state.inventory).forget(device_id)?;
    }
    // A connected device reappears immediately as a new entry.
    refresh(app).await
}

// --- export / import -------------------------------------------------------

#[tauri::command]
pub async fn export_inventory(app: AppHandle) -> CmdResult<Option<String>> {
    let (json, count) = {
        let state = app.state::<AppState>();
        let inventory = lock(&state.inventory);
        let doc = inventory.export(&state.version, hostname().as_deref())?;
        (doc.to_json_pretty(), doc.devices.len())
    };
    let dialog_app = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_title("Export port mappings")
            .add_filter("ComInspect inventory", &["json"])
            .set_file_name(DEFAULT_EXPORT_FILE_NAME)
            .blocking_save_file()
    })
    .await
    .map_err(|e| CommandError::msg(e.to_string()))?;
    let Some(path) = chosen else { return Ok(None) };
    let path: PathBuf = path
        .into_path()
        .map_err(|e| CommandError::msg(e.to_string()))?;
    std::fs::write(&path, json)?;
    log::info!("exported {count} devices to {}", path.display());
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub async fn import_inventory(app: AppHandle, mode: ImportMode) -> CmdResult<Option<ImportReport>> {
    let dialog_app = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_title("Import port mappings")
            .add_filter("ComInspect inventory", &["json"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| CommandError::msg(e.to_string()))?;
    let Some(path) = chosen else { return Ok(None) };
    let path: PathBuf = path
        .into_path()
        .map_err(|e| CommandError::msg(e.to_string()))?;
    let metadata = std::fs::metadata(&path)?;
    if metadata.len() > 20 * 1024 * 1024 {
        return Err(CommandError::msg(
            "The file is too large to be a ComInspect inventory.",
        ));
    }
    let json = std::fs::read_to_string(&path)?;
    let doc = ExportDocument::from_json(&json).map_err(|e| CommandError::msg(e.to_string()))?;
    let report = {
        let state = app.state::<AppState>();
        let mut inventory = lock(&state.inventory);
        if inventory.store().path().is_some() {
            let _ = inventory.store().create_backup("before-import");
        }
        inventory.import(&doc, mode, hostname().as_deref())?
    };
    log::info!("imported {} ({report:?})", path.display());
    // Re-match immediately so imported identities attach to connected devices.
    refresh(app).await?;
    Ok(Some(report))
}

// --- application info & settings ---------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub identifier: String,
    pub data_dir: String,
    pub db_path: String,
    pub log_dir: String,
    pub backups_dir: Option<String>,
    pub schema_version: u32,
    pub device_count: i64,
    pub key_count: i64,
    pub event_count: i64,
    pub storage: StorageInfo,
    pub database_backup: Option<String>,
    pub recovered_file: Option<String>,
    pub migrations_applied: Vec<u32>,
    pub startup: StartupInfo,
    pub repository: String,
    pub updates: UpdateStatus,
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> CmdResult<AppInfo> {
    let inventory = lock(&state.inventory);
    let (device_count, key_count, event_count) = inventory.stats()?;
    let report = lock(&state.open_report).clone();
    Ok(AppInfo {
        version: state.version.clone(),
        platform: cominspect_platform::platform_name().into(),
        arch: std::env::consts::ARCH.into(),
        identifier: app.config().identifier.clone(),
        data_dir: state.paths.data_dir.display().to_string(),
        db_path: state.paths.db_path.display().to_string(),
        log_dir: state.paths.log_dir.display().to_string(),
        backups_dir: inventory
            .store()
            .backups_dir()
            .map(|p| p.display().to_string()),
        schema_version: inventory.store().schema_version()?,
        device_count,
        key_count,
        event_count,
        storage: StorageInfo {
            temporary: inventory.store().temporary_reason().is_some(),
            reason: inventory.store().temporary_reason().map(str::to_string),
        },
        database_backup: report.backup.map(|p| p.display().to_string()),
        recovered_file: report
            .recovered_corrupt_file
            .map(|p| p.display().to_string()),
        migrations_applied: report.migrations_applied,
        startup: state.startup.clone(),
        repository: updates::REPOSITORY.into(),
        updates: state.updates.status(),
    })
}

/// Called by the UI after it rendered the first inventory (launch
/// confirmation for the update recovery logic).
#[tauri::command]
pub async fn app_ready(state: State<'_, AppState>) -> CmdResult<()> {
    let inventory = lock(&state.inventory);
    lifecycle::confirm_ready(inventory.store(), &state.version)?;
    Ok(())
}

#[tauri::command]
pub async fn get_ui_prefs(state: State<'_, AppState>) -> CmdResult<serde_json::Value> {
    Ok(lock(&state.inventory)
        .store()
        .get_setting::<serde_json::Value>("ui.prefs")?
        .unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
pub async fn set_ui_prefs(state: State<'_, AppState>, prefs: serde_json::Value) -> CmdResult<()> {
    if prefs.to_string().len() > 32 * 1024 {
        return Err(CommandError::msg("preferences are too large"));
    }
    lock(&state.inventory)
        .store()
        .set_setting("ui.prefs", &prefs)?;
    Ok(())
}

#[tauri::command]
pub fn log_from_ui(level: String, message: String) {
    let message: String = message.chars().take(2000).collect();
    match level.as_str() {
        "error" => log::error!(target: "ui", "{message}"),
        "warn" => log::warn!(target: "ui", "{message}"),
        _ => log::info!(target: "ui", "{message}"),
    }
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    Data,
    Logs,
    Backups,
}

#[tauri::command]
pub async fn open_location(app: AppHandle, which: Location) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let path = match which {
        Location::Data => state.paths.data_dir.clone(),
        Location::Logs => state.paths.log_dir.clone(),
        Location::Backups => cominspect_store::backups_dir_for(&state.paths.db_path),
    };
    std::fs::create_dir_all(&path)?;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| CommandError::msg(e.to_string()))
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    Homepage,
    Releases,
    Issues,
}

#[tauri::command]
pub async fn open_link(app: AppHandle, which: Link, version: Option<String>) -> CmdResult<()> {
    let base = updates::REPOSITORY;
    let url = match (which, version) {
        (Link::Homepage, _) => base.to_string(),
        (Link::Releases, Some(v))
            if v.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') =>
        {
            format!("{base}/releases/tag/v{v}")
        }
        (Link::Releases, _) => format!("{base}/releases"),
        (Link::Issues, _) => format!("{base}/issues"),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| CommandError::msg(e.to_string()))
}

// --- updates ------------------------------------------------------------------

#[tauri::command]
pub async fn get_update_status(state: State<'_, AppState>) -> CmdResult<UpdateStatus> {
    Ok(state.updates.status())
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> CmdResult<UpdateStatus> {
    Ok(updates::check(&app).await)
}

#[tauri::command]
pub async fn install_update(app: AppHandle, on_event: Channel<DownloadEvent>) -> CmdResult<()> {
    updates::install(&app, on_event).await
}

#[tauri::command]
pub async fn reinstall_version(
    app: AppHandle,
    version: String,
    on_event: Channel<DownloadEvent>,
) -> CmdResult<()> {
    updates::reinstall_version(&app, version, on_event).await
}

#[tauri::command]
pub async fn set_auto_update_check(app: AppHandle, enabled: bool) -> CmdResult<UpdateStatus> {
    updates::set_auto_check(&app, enabled)
}

#[tauri::command]
pub async fn set_update_channel(app: AppHandle, channel: UpdateChannel) -> CmdResult<UpdateStatus> {
    updates::set_channel(&app, channel)
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    crate::state::stop_monitor(&app);
    app.restart();
}

// --- backups ------------------------------------------------------------------

#[tauri::command]
pub async fn list_database_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupInfo>> {
    Ok(list_backups(&state.paths.db_path)?)
}

#[tauri::command]
pub async fn create_database_backup(state: State<'_, AppState>) -> CmdResult<String> {
    let inventory = lock(&state.inventory);
    Ok(inventory
        .store()
        .create_backup("manual")?
        .display()
        .to_string())
}

/// Replaces the database with a backup (only files from the backups
/// directory are accepted) and rescans.
#[tauri::command]
pub async fn restore_database_backup(
    app: AppHandle,
    file_name: String,
) -> CmdResult<InventoryView> {
    let state = app.state::<AppState>();
    let backups = list_backups(&state.paths.db_path)?;
    let backup = backups
        .into_iter()
        .find(|b| b.file_name == file_name)
        .ok_or_else(|| CommandError::msg("That backup was not found."))?;
    {
        let mut inventory = lock(&state.inventory);
        // Close the current database before replacing the file.
        let placeholder = Store::open_temporary(&state.version, "restoring a backup".into())?;
        let old = std::mem::replace(
            &mut *inventory,
            Inventory::new(placeholder, cominspect_platform::platform_name()),
        );
        drop(old.into_store());
        cominspect_store::restore_backup(&state.paths.db_path, &backup.path)?;
        let (store, report) = Store::open(&state.paths.db_path, &state.version)?;
        *lock(&state.open_report) = report;
        *inventory = Inventory::new(store, cominspect_platform::platform_name());
    }
    log::info!("database restored from {}", backup.path.display());
    refresh(app).await
}

// --- diagnostics --------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeCatalog {
    pub probes: Vec<CatProbeInfo>,
    pub icom_addresses: Vec<(u8, String)>,
    pub max_ptt_ms: u64,
}

#[tauri::command]
pub fn diag_probe_catalog() -> ProbeCatalog {
    ProbeCatalog {
        probes: cat::PROBES.to_vec(),
        icom_addresses: cat::ICOM_ADDRESSES
            .iter()
            .map(|(a, n)| (*a, (*n).to_string()))
            .collect(),
        max_ptt_ms: diagnostics::MAX_PTT_MS,
    }
}

fn check_port_name(port: &str) -> CmdResult<()> {
    let ok = !port.is_empty()
        && port.len() < 256
        && (cominspect_core::model::com_number(port).is_some() || port.starts_with("/dev/"))
        && !port.contains("..");
    if ok {
        Ok(())
    } else {
        Err(CommandError::msg("invalid port name"))
    }
}

#[tauri::command]
pub async fn diag_open_test(port: String) -> CmdResult<OpenTestReport> {
    check_port_name(&port)?;
    log::info!("diagnostics: open test on {port}");
    tauri::async_runtime::spawn_blocking(move || diagnostics::open_test(&port))
        .await
        .map_err(|e| CommandError::msg(e.to_string()))
}

#[tauri::command]
pub async fn diag_cat_query(
    port: String,
    settings: SerialSettings,
    protocol: CatProtocol,
    civ_address: u8,
    timeout_ms: u64,
) -> CmdResult<CatQueryReport> {
    check_port_name(&port)?;
    log::info!(
        "diagnostics: CAT query {protocol:?} on {port} at {} baud",
        settings.baud_rate
    );
    tauri::async_runtime::spawn_blocking(move || {
        diagnostics::cat_query(
            &port,
            &settings,
            protocol,
            civ_address,
            Duration::from_millis(timeout_ms),
        )
    })
    .await
    .map_err(|e| CommandError::msg(e.to_string()))
}

#[tauri::command]
pub async fn diag_ptt_test(
    port: String,
    line: ControlLine,
    duration_ms: u64,
) -> CmdResult<PttTestReport> {
    check_port_name(&port)?;
    log::warn!(
        "diagnostics: PTT test on {port} via {line:?} for {duration_ms} ms (user confirmed)"
    );
    tauri::async_runtime::spawn_blocking(move || {
        diagnostics::ptt_test(&port, line, Duration::from_millis(duration_ms))
    })
    .await
    .map_err(|e| CommandError::msg(e.to_string()))
}

// --- port usage ("in use by", notify when free) --------------------------------

fn program_setting(device_id: i64) -> String {
    format!("usage.program.{device_id}")
}

#[tauri::command]
pub fn get_port_usage(state: State<'_, AppState>) -> UsageView {
    state.usage.view()
}

/// Waits for `port` to be released; optionally starts `then_open` then. The
/// program is remembered for the device.
#[tauri::command]
pub async fn watch_port(
    state: State<'_, AppState>,
    port: String,
    device_id: Option<i64>,
    then_open: Option<String>,
) -> CmdResult<UsageView> {
    check_port_name(&port)?;
    let then_open = then_open.filter(|p| !p.trim().is_empty());
    if let Some(program) = &then_open
        && !std::path::Path::new(program).exists()
    {
        return Err(CommandError::msg(format!("{program} was not found.")));
    }
    if let Some(id) = device_id {
        let inventory = lock(&state.inventory);
        let key = program_setting(id);
        let _ = match &then_open {
            Some(program) => inventory.store().set_setting(&key, program),
            None => inventory.store().delete_setting(&key),
        };
    }
    log::info!(
        "waiting for {port} to be released{}",
        then_open
            .as_deref()
            .map(|p| format!(", then starting {p}"))
            .unwrap_or_default()
    );
    state
        .usage
        .watch(&port, then_open)
        .map_err(CommandError::msg)
}

#[tauri::command]
pub fn unwatch_port(state: State<'_, AppState>, port: String) -> UsageView {
    state.usage.unwatch(&port)
}

/// The program last chosen to start when this device's port is free.
#[tauri::command]
pub async fn get_watch_program(
    state: State<'_, AppState>,
    device_id: i64,
) -> CmdResult<Option<String>> {
    let inventory = lock(&state.inventory);
    Ok(inventory
        .store()
        .get_setting::<String>(&program_setting(device_id))?
        .filter(|p| std::path::Path::new(p).exists()))
}

// --- VSPE ----------------------------------------------------------------------

/// The VSPE configuration, from wherever the user asked ComInspect to read it.
#[tauri::command]
pub async fn get_vspe(state: State<'_, AppState>) -> CmdResult<VspeView> {
    let source = {
        let inventory = lock(&state.inventory);
        VspeSource::load(inventory.store())?
    };
    Ok(vspe::view(source))
}

fn use_vspe_source(state: &AppState, source: VspeSource) -> CmdResult<VspeView> {
    {
        let inventory = lock(&state.inventory);
        source.save(inventory.store())?;
    }
    log::info!("VSPE configuration source: {source:?}");
    Ok(vspe::view(source))
}

/// Picks a `.vspe` file to read instead of VSPE's startup configuration.
/// `None` when the dialog was cancelled.
#[tauri::command]
pub async fn choose_vspe_file(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<VspeView>> {
    let dialog_app = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_title("VSPE configuration file")
            .add_filter("VSPE configuration", &["vspe"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| CommandError::msg(e.to_string()))?;
    let Some(path) = chosen else { return Ok(None) };
    let path: PathBuf = path
        .into_path()
        .map_err(|e| CommandError::msg(e.to_string()))?;
    // Refuse files that are not VSPE configurations.
    cominspect_platform::vspe::read_config(&path).map_err(CommandError::msg)?;
    let path = path.display().to_string();
    use_vspe_source(&state, VspeSource::File { path }).map(Some)
}

/// Picks a folder whose newest `.vspe` file is read. `None` when the dialog
/// was cancelled.
#[tauri::command]
pub async fn choose_vspe_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<VspeView>> {
    let dialog_app = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_title("Folder with your VSPE configuration files")
            .blocking_pick_folder()
    })
    .await
    .map_err(|e| CommandError::msg(e.to_string()))?;
    let Some(path) = chosen else { return Ok(None) };
    let path: PathBuf = path
        .into_path()
        .map_err(|e| CommandError::msg(e.to_string()))?;
    if cominspect_platform::vspe::newest_config_in(&path).is_none() {
        return Err(CommandError::msg(format!(
            "There are no VSPE configuration files (.vspe) in {}.",
            path.display()
        )));
    }
    let path = path.display().to_string();
    use_vspe_source(&state, VspeSource::Folder { path }).map(Some)
}

/// Goes back to VSPE's startup configuration.
#[tauri::command]
pub async fn use_vspe_autostart(state: State<'_, AppState>) -> CmdResult<VspeView> {
    use_vspe_source(&state, VspeSource::Autostart)
}

#[tauri::command]
pub async fn pick_program(app: AppHandle) -> CmdResult<Option<String>> {
    let dialog_app = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        let dialog = dialog_app
            .dialog()
            .file()
            .set_title("Program to start when the port is free");
        let dialog = if cfg!(windows) {
            dialog.add_filter("Programs and shortcuts", &["exe", "lnk", "bat", "cmd"])
        } else if cfg!(target_os = "macos") {
            dialog.add_filter("Applications", &["app"])
        } else {
            dialog
        };
        dialog.blocking_pick_file()
    })
    .await
    .map_err(|e| CommandError::msg(e.to_string()))?;
    let Some(path) = chosen else { return Ok(None) };
    let path: PathBuf = path
        .into_path()
        .map_err(|e| CommandError::msg(e.to_string()))?;
    Ok(Some(path.display().to_string()))
}
