//! Application updates through `tauri-plugin-updater`.
//!
//! * Update artifacts are signed with the project's minisign key; the public
//!   key is embedded in `tauri.conf.json` and verification cannot be skipped.
//!   `requireSignedVersion` additionally binds each artifact to the version
//!   the feed announces, preventing downgrade attacks.
//! * Feeds are static JSON files on GitHub Releases. Stable uses
//!   `releases/latest/download/latest.json` (GitHub never marks drafts or
//!   pre-releases as "latest"); beta uses a feed maintained by the release
//!   workflow and is only used when the user explicitly opts in.
//! * Rollback reinstalls the previous release through that release's own
//!   signed `latest.json`, so it goes through the same verification.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, Url};

use cominspect_core::time::now_ms;
use cominspect_store::Store;

use crate::error::{CmdResult, CommandError};
use crate::state::{AppState, lock};

pub const REPOSITORY: &str = "https://github.com/KK4ODA/ComInspect";
pub const EVENT_UPDATE_STATUS: &str = "updater://status";
const CHECK_INTERVAL_MS: i64 = 20 * 3600 * 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl UpdateChannel {
    pub fn endpoints(self) -> Vec<String> {
        let stable = format!("{REPOSITORY}/releases/latest/download/latest.json");
        match self {
            UpdateChannel::Stable => vec![stable],
            UpdateChannel::Beta => vec![
                format!("{REPOSITORY}/releases/download/updater-feeds/beta.json"),
                stable,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Installing,
    ReadyToRestart,
    Error,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub date: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    /// This build carries an update verification key.
    pub configured: bool,
    /// The app was installed by an installer the updater can replace
    /// (false for development builds and unpackaged binaries).
    pub supported: bool,
    pub phase: UpdatePhase,
    pub available: Option<AvailableUpdate>,
    pub last_checked: Option<i64>,
    pub error: Option<String>,
    pub channel: UpdateChannel,
    pub auto_check: bool,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Progress {
        downloaded: u64,
        total: Option<u64>,
    },
    Installing,
    Finished,
}

pub struct UpdateState {
    status: Mutex<UpdateStatus>,
    pending: Mutex<Option<tauri_plugin_updater::Update>>,
    busy: tokio::sync::Mutex<()>,
}

fn pubkey_configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| k.trim().len() > 40)
}

impl UpdateState {
    pub fn load(app: &AppHandle, store: &Store, version: &str) -> UpdateState {
        let status = UpdateStatus {
            current_version: version.to_string(),
            configured: pubkey_configured(app),
            supported: tauri::utils::platform::bundle_type().is_some()
                && tauri_plugin_updater::target().is_some(),
            channel: store
                .get_setting::<UpdateChannel>("update.channel")
                .ok()
                .flatten()
                .unwrap_or_default(),
            auto_check: store
                .get_setting::<bool>("update.autoCheck")
                .ok()
                .flatten()
                .unwrap_or(true),
            last_checked: store
                .get_setting::<i64>("update.lastChecked")
                .ok()
                .flatten(),
            ..Default::default()
        };
        UpdateState {
            status: Mutex::new(status),
            pending: Mutex::new(None),
            busy: tokio::sync::Mutex::new(()),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        lock(&self.status).clone()
    }
}

fn set_status(app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) -> UpdateStatus {
    let state = app.state::<AppState>();
    let status = {
        let mut status = lock(&state.updates.status);
        f(&mut status);
        status.clone()
    };
    let _ = app.emit(EVENT_UPDATE_STATUS, &status);
    status
}

fn friendly_error(e: &tauri_plugin_updater::Error) -> String {
    let text = e.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("signature") {
        format!("The update could not be verified and was not installed ({text}).")
    } else if lower.contains("dns")
        || lower.contains("connect")
        || lower.contains("timed out")
        || lower.contains("network")
    {
        "Could not reach the update server. Check your internet connection and try again.".into()
    } else if lower.contains("404") || lower.contains("not found") || lower.contains("release") {
        "No release information is published yet.".into()
    } else {
        text
    }
}

fn updater(app: &AppHandle, endpoints: Vec<String>) -> CmdResult<tauri_plugin_updater::Updater> {
    use tauri_plugin_updater::UpdaterExt;
    let urls = endpoints
        .iter()
        .map(|e| Url::parse(e).map_err(|err| CommandError::msg(err.to_string())))
        .collect::<CmdResult<Vec<_>>>()?;
    app.updater_builder()
        .endpoints(urls)
        .map_err(|e| CommandError::msg(e.to_string()))?
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| CommandError::msg(e.to_string()))
}

/// Checks the configured channel for a newer release.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let state = app.state::<AppState>();
    let Ok(_guard) = state.updates.busy.try_lock() else {
        return state.updates.status();
    };
    let current = state.updates.status();
    if !current.configured {
        return set_status(app, |s| {
            s.phase = UpdatePhase::Error;
            s.error = Some("This build has no update key, so updates are disabled.".into());
        });
    }
    set_status(app, |s| {
        s.phase = UpdatePhase::Checking;
        s.error = None;
    });
    let result = match updater(app, current.channel.endpoints()) {
        Ok(u) => u.check().await.map_err(|e| friendly_error(&e)),
        Err(e) => Err(e.to_string()),
    };
    let checked_at = now_ms();
    {
        let inventory = lock(&state.inventory);
        let _ = inventory
            .store()
            .set_setting("update.lastChecked", &checked_at);
    }
    match result {
        Ok(Some(update)) => {
            let available = AvailableUpdate {
                version: update.version.clone(),
                date: update
                    .raw_json
                    .get("pub_date")
                    .and_then(|d| d.as_str())
                    .map(str::to_string),
                notes: update.body.clone(),
            };
            log::info!("update available: {}", available.version);
            *lock(&state.updates.pending) = Some(update);
            set_status(app, |s| {
                s.phase = UpdatePhase::Available;
                s.available = Some(available);
                s.last_checked = Some(checked_at);
            })
        }
        Ok(None) => set_status(app, |s| {
            s.phase = UpdatePhase::UpToDate;
            s.available = None;
            s.last_checked = Some(checked_at);
        }),
        Err(error) => {
            log::warn!("update check failed: {error}");
            set_status(app, |s| {
                s.phase = UpdatePhase::Error;
                s.error = Some(error);
                s.last_checked = Some(checked_at);
            })
        }
    }
}

async fn download_and_install(
    app: &AppHandle,
    update: tauri_plugin_updater::Update,
    on_event: &Channel<DownloadEvent>,
) -> CmdResult<()> {
    // Safety net before replacing the application.
    {
        let state = app.state::<AppState>();
        let inventory = lock(&state.inventory);
        if inventory.store().path().is_some() {
            match inventory
                .store()
                .create_backup(&format!("before-{}", update.version))
            {
                Ok(path) => log::info!("pre-update backup: {}", path.display()),
                Err(e) => log::warn!("pre-update backup failed: {e}"),
            }
        }
    }
    set_status(app, |s| {
        s.phase = UpdatePhase::Downloading;
        s.downloaded = 0;
        s.total = None;
        s.error = None;
    });
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    let progress_app = app.clone();
    let result = update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_event.send(DownloadEvent::Progress { downloaded, total });
                if downloaded - last_emit > 256 * 1024 || Some(downloaded) == total {
                    last_emit = downloaded;
                    set_status(&progress_app, |s| {
                        s.downloaded = downloaded;
                        s.total = total;
                    });
                }
            },
            || {
                let _ = on_event.send(DownloadEvent::Installing);
                set_status(&progress_app, |s| s.phase = UpdatePhase::Installing);
            },
        )
        .await;
    match result {
        Ok(()) => {
            let _ = on_event.send(DownloadEvent::Finished);
            log::info!("update to {} installed", update.version);
            set_status(app, |s| s.phase = UpdatePhase::ReadyToRestart);
            Ok(())
        }
        Err(e) => {
            let message = friendly_error(&e);
            log::error!("update failed: {e}");
            set_status(app, |s| {
                s.phase = UpdatePhase::Error;
                s.error = Some(message.clone());
            });
            Err(CommandError::msg(message))
        }
    }
}

/// Downloads, verifies and installs the update found by the last check.
/// On Windows the installer closes the app and restarts it.
pub async fn install(app: &AppHandle, on_event: Channel<DownloadEvent>) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard = state.updates.busy.lock().await;
    let Some(update) = lock(&state.updates.pending).take() else {
        return Err(CommandError::msg(
            "No update is pending. Check for updates first.",
        ));
    };
    download_and_install(app, update, &on_event).await
}

/// Reinstalls a previous release (user-initiated recovery).
pub async fn reinstall_version(
    app: &AppHandle,
    version: String,
    on_event: Channel<DownloadEvent>,
) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard = state.updates.busy.lock().await;
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Err(CommandError::msg("invalid version"));
    }
    let wanted = version.clone();
    use tauri_plugin_updater::UpdaterExt;
    let url = Url::parse(&format!(
        "{REPOSITORY}/releases/download/v{version}/latest.json"
    ))
    .map_err(|e| CommandError::msg(e.to_string()))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| CommandError::msg(e.to_string()))?
        .version_comparator(move |_current, release| release.version.to_string() == wanted)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| CommandError::msg(e.to_string()))?;
    let update = updater
        .check()
        .await
        .map_err(|e| CommandError::msg(friendly_error(&e)))?
        .ok_or_else(|| {
            CommandError::msg(format!("Version {version} is not available for download."))
        })?;
    download_and_install(app, update, &on_event).await
}

pub fn set_auto_check(app: &AppHandle, enabled: bool) -> CmdResult<UpdateStatus> {
    let state = app.state::<AppState>();
    lock(&state.inventory)
        .store()
        .set_setting("update.autoCheck", &enabled)?;
    Ok(set_status(app, |s| s.auto_check = enabled))
}

pub fn set_channel(app: &AppHandle, channel: UpdateChannel) -> CmdResult<UpdateStatus> {
    let state = app.state::<AppState>();
    lock(&state.inventory)
        .store()
        .set_setting("update.channel", &channel)?;
    *lock(&state.updates.pending) = None;
    Ok(set_status(app, |s| {
        s.channel = channel;
        s.available = None;
        s.phase = UpdatePhase::Idle;
    }))
}

/// Background checks: shortly after startup, then about once a day. Never
/// blocks startup; failures are only logged.
pub fn spawn_background_checks(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            let status = app.state::<AppState>().updates.status();
            let due = status
                .last_checked
                .is_none_or(|t| now_ms() - t > CHECK_INTERVAL_MS);
            if status.auto_check
                && status.configured
                && status.supported
                && due
                && !matches!(
                    status.phase,
                    UpdatePhase::Downloading
                        | UpdatePhase::Installing
                        | UpdatePhase::ReadyToRestart
                )
            {
                check(&app).await;
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_never_mix_silently() {
        let stable = UpdateChannel::Stable.endpoints();
        assert_eq!(stable.len(), 1);
        assert!(stable[0].ends_with("/releases/latest/download/latest.json"));
        let beta = UpdateChannel::Beta.endpoints();
        assert!(beta[0].ends_with("/updater-feeds/beta.json"));
        assert_eq!(UpdateChannel::default(), UpdateChannel::Stable);
    }
}
