//! Which programs have the connected ports open, and "tell me when this port
//! is free". A background thread checks the connected ports (never opening
//! them, see `cominspect_platform::usage`) while the window is visible or a
//! port is being watched, and tells the UI when anything changes.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use cominspect_core::time::now_ms;
use cominspect_core::usage::{PortUsage, UsageSnapshot, changes};
use cominspect_platform::usage::UsageProbe;

use crate::state::{AppState, lock};

pub const EVENT_USAGE_UPDATED: &str = "usage://updated";
pub const EVENT_USAGE_RELEASED: &str = "usage://released";

/// Checks while someone waits for a port.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);
/// Checks while the window is visible.
const VISIBLE_INTERVAL: Duration = Duration::from_secs(2);
/// A change seen within this long of the previous check happened at most
/// this long before it was seen.
const CONTINUOUS_MS: i64 = 10_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchView {
    pub armed_at: i64,
    /// Program to start once the port is free.
    pub then_open: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortUsageView {
    #[serde(flatten)]
    pub usage: PortUsage,
    /// When the port was first seen in its current state (ms since the
    /// epoch).
    pub since: i64,
    /// True when ComInspect saw the change happen; false when the port was
    /// already in this state when it was first checked, or after a pause in
    /// checking (the change happened at `since` or earlier).
    pub since_exact: bool,
    pub watch: Option<WatchView>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageView {
    /// Keyed by port name.
    pub ports: BTreeMap<String, PortUsageView>,
    /// What the check cannot see (e.g. programs running as administrator).
    pub limitation: Option<String>,
    pub checked_at: Option<i64>,
}

/// Sent to the UI when a watched port is released.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasedEvent {
    pub port: String,
    pub message: String,
    pub started: Option<String>,
    pub error: Option<String>,
}

struct Watch {
    armed_at: i64,
    then_open: Option<String>,
    /// The programs last seen holding the port, for the release message.
    holders: Option<PortUsage>,
}

#[derive(Clone, Copy)]
struct Since {
    at: i64,
    exact: bool,
}

#[derive(Default)]
struct Inner {
    /// Connected ports and the name to use for each in messages.
    ports: BTreeMap<String, String>,
    snapshot: Option<UsageSnapshot>,
    checked_at: Option<i64>,
    since: HashMap<String, Since>,
    watches: HashMap<String, Watch>,
    /// Set to check again right away.
    poke: bool,
    stop: bool,
}

#[derive(Default)]
pub struct UsageState {
    inner: Mutex<Inner>,
    wake: Condvar,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl UsageState {
    fn poke(&self) {
        lock(&self.inner).poke = true;
        self.wake.notify_all();
    }

    /// Sets the connected ports (from the latest scan). Watches on ports
    /// that disappeared end with a notice.
    pub fn set_ports(&self, app: &AppHandle, ports: BTreeMap<String, String>) {
        let mut gone = Vec::new();
        {
            let mut inner = lock(&self.inner);
            if inner.ports == ports {
                return;
            }
            let watched: Vec<String> = inner.watches.keys().cloned().collect();
            for port in watched {
                if !ports.contains_key(&port) {
                    inner.watches.remove(&port);
                    let label = inner.ports.get(&port).cloned().unwrap_or_default();
                    gone.push((port, label));
                }
            }
            inner.since.retain(|port, _| ports.contains_key(port));
            // A port that comes back is checked afresh.
            if let Some(snapshot) = inner.snapshot.as_mut() {
                snapshot.ports.retain(|port, _| ports.contains_key(port));
            }
            inner.ports = ports;
            inner.poke = true;
        }
        self.wake.notify_all();
        for (port, label) in gone {
            let message = format!(
                "{} was disconnected, so ComInspect stopped waiting for it.",
                name_with_port(&label, &port)
            );
            notify(app, &format!("{port} disconnected"), &message);
            let _ = app.emit(
                EVENT_USAGE_RELEASED,
                ReleasedEvent {
                    port,
                    message,
                    started: None,
                    error: None,
                },
            );
        }
        let _ = app.emit(EVENT_USAGE_UPDATED, self.view());
    }

    pub fn view(&self) -> UsageView {
        let inner = lock(&self.inner);
        view_of(&inner)
    }

    /// Starts waiting for `port` to be released.
    pub fn watch(&self, port: &str, then_open: Option<String>) -> Result<UsageView, String> {
        {
            let mut inner = lock(&self.inner);
            if !inner.ports.contains_key(port) {
                return Err(format!("{port} is not connected."));
            }
            let current = inner
                .snapshot
                .as_ref()
                .and_then(|s| s.ports.get(port))
                .cloned();
            if matches!(current, Some(PortUsage::Free)) {
                return Err(format!("{port} is already free."));
            }
            inner.watches.insert(
                port.to_string(),
                Watch {
                    armed_at: now_ms(),
                    then_open: then_open.filter(|p| !p.trim().is_empty()),
                    holders: current.filter(PortUsage::is_in_use),
                },
            );
        }
        self.poke();
        Ok(self.view())
    }

    pub fn unwatch(&self, port: &str) -> UsageView {
        lock(&self.inner).watches.remove(port);
        self.view()
    }

    /// Checks again now (e.g. when the window becomes visible).
    pub fn refresh(&self) {
        self.poke();
    }
}

fn name_with_port(label: &str, port: &str) -> String {
    if label.is_empty() {
        port.to_string()
    } else {
        format!("{label} ({port})")
    }
}

fn view_of(inner: &Inner) -> UsageView {
    let mut view = UsageView {
        checked_at: inner.checked_at,
        limitation: inner.snapshot.as_ref().and_then(|s| s.limitation.clone()),
        ..Default::default()
    };
    if let Some(snapshot) = &inner.snapshot {
        for (port, usage) in &snapshot.ports {
            if !inner.ports.contains_key(port) {
                continue;
            }
            let since = inner.since.get(port).copied().unwrap_or(Since {
                at: now_ms(),
                exact: false,
            });
            view.ports.insert(
                port.clone(),
                PortUsageView {
                    usage: usage.clone(),
                    since: since.at,
                    since_exact: since.exact,
                    watch: inner.watches.get(port).map(|w| WatchView {
                        armed_at: w.armed_at,
                        then_open: w.then_open.clone(),
                    }),
                },
            );
        }
    }
    view
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("could not show a notification: {e}");
    }
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false))
}

/// A watched port is free: notify, and start the chosen program.
fn announce_release(app: &AppHandle, port: &str, label: &str, waited_ms: i64, watch: Watch) {
    let who = watch
        .holders
        .as_ref()
        .map(PortUsage::holder_names)
        .unwrap_or_default();
    let seconds = (waited_ms.max(0) as f64 / 1000.0).round() as i64;
    let mut message = format!(
        "{} is free: {} released it{}.",
        name_with_port(label, port),
        if who.is_empty() { "the program" } else { &who },
        if seconds >= 2 {
            format!(" after {seconds} s")
        } else {
            String::new()
        }
    );
    let mut started = None;
    let mut error = None;
    if let Some(program) = &watch.then_open {
        let name = std::path::Path::new(program)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| program.clone());
        match cominspect_platform::launch::launch_program(program) {
            Ok(()) => {
                log::info!("started {program} after {port} was released");
                message.push_str(&format!(" Starting {name}."));
                started = Some(name);
            }
            Err(e) => {
                log::warn!("could not start {program}: {e}");
                message.push_str(&format!(" {name} could not be started: {e}"));
                error = Some(e);
            }
        }
    }
    log::info!("{message}");
    notify(app, &format!("{port} is free"), &message);
    let _ = app.emit(
        EVENT_USAGE_RELEASED,
        ReleasedEvent {
            port: port.to_string(),
            message,
            started,
            error,
        },
    );
}

pub fn start(app: &AppHandle) {
    let handle = app.clone();
    let thread = std::thread::Builder::new()
        .name("cominspect-usage".into())
        .spawn(move || run(handle))
        .ok();
    let state = app.state::<AppState>();
    *lock(&state.usage.thread) = thread;
}

pub fn stop(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        lock(&state.usage.inner).stop = true;
        state.usage.wake.notify_all();
        if let Some(thread) = lock(&state.usage.thread).take() {
            let _ = thread.join();
        }
    }
}

fn run(app: AppHandle) {
    let mut probe = UsageProbe::new();
    loop {
        let state = app.state::<AppState>();
        let usage = &state.usage;
        let (ports, watching) = {
            let inner = lock(&usage.inner);
            if inner.stop {
                return;
            }
            (
                inner.ports.keys().cloned().collect::<Vec<_>>(),
                !inner.watches.is_empty(),
            )
        };
        if !ports.is_empty() && (watching || window_visible(&app)) {
            let snapshot = probe.check(&ports);
            if snapshot.duration_ms > 250 {
                log::debug!("port usage check took {} ms", snapshot.duration_ms);
            }
            let mut released = Vec::new();
            let changed = {
                let mut inner = lock(&usage.inner);
                let previous = inner.snapshot.clone().unwrap_or_default();
                let now = now_ms();
                let continuous = inner.checked_at.is_some_and(|t| now - t <= CONTINUOUS_MS);
                let diff = changes(&previous, &snapshot);
                for change in &diff {
                    let exact = continuous && change.before.is_some();
                    inner
                        .since
                        .insert(change.port.clone(), Since { at: now, exact });
                }
                // A watched port that is free now ends its watch, however it
                // got there (even if it was never seen in use).
                let watched: Vec<String> = inner.watches.keys().cloned().collect();
                for port in watched {
                    match snapshot.ports.get(&port) {
                        Some(PortUsage::Free) => {
                            if let Some(watch) = inner.watches.remove(&port) {
                                let label = inner.ports.get(&port).cloned().unwrap_or_default();
                                released.push((port, label, now - watch.armed_at, watch));
                            }
                        }
                        Some(in_use @ PortUsage::InUse { .. }) => {
                            if let Some(watch) = inner.watches.get_mut(&port) {
                                watch.holders = Some(in_use.clone());
                            }
                        }
                        _ => {}
                    }
                }
                let changed = !diff.is_empty()
                    || inner.snapshot.as_ref().map(|s| &s.limitation) != Some(&snapshot.limitation)
                    || inner.snapshot.is_none();
                inner.snapshot = Some(snapshot);
                inner.checked_at = Some(now);
                changed
            };
            for (port, label, waited, watch) in released {
                announce_release(&app, &port, &label, waited, watch);
            }
            if changed {
                let _ = app.emit(EVENT_USAGE_UPDATED, usage.view());
            }
        }

        let interval = if watching {
            WATCH_INTERVAL
        } else {
            VISIBLE_INTERVAL
        };
        let mut inner = lock(&usage.inner);
        if !inner.poke && !inner.stop {
            inner = usage
                .wake
                .wait_timeout(inner, interval)
                .map(|(guard, _)| guard)
                .unwrap_or_else(|poisoned| poisoned.into_inner().0);
        }
        inner.poke = false;
    }
}
