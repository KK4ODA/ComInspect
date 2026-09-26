//! Launch bookkeeping: detects upgrades, records update history and
//! confirms that a new version started successfully.
//!
//! At startup the version is written to `launch_pending_version`. Once the UI
//! has rendered the first inventory it calls `app_ready`, which records the
//! version as `last_good_version`. If a version's previous launch never got
//! that far, the UI shows a recovery banner offering the logs and a
//! reinstall of the last good version.

use serde::Serialize;

use cominspect_core::time::now_ms;
use cominspect_store::{Result, Store};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    pub failed_version: String,
    pub last_good_version: Option<String>,
    pub attempts: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupInfo {
    pub version: String,
    /// Previous version when this is the first launch after an update.
    pub updated_from: Option<String>,
    pub first_run: bool,
    pub recovery: Option<RecoveryInfo>,
    /// Most recent version this installation was updated from (used to offer
    /// a rollback).
    pub previous_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecord {
    pub from: String,
    pub to: String,
    pub at: i64,
}

pub fn record_launch(store: &Store, version: &str) -> Result<StartupInfo> {
    let last_app = store.get_meta("last_app_version")?;
    let last_good = store.get_meta("last_good_version")?;
    let pending = store.get_meta("launch_pending_version")?;
    let attempts: u32 = store
        .get_meta("launch_pending_attempts")?
        .and_then(|a| a.parse().ok())
        .unwrap_or(0);

    let retry_of_pending = pending.as_deref() == Some(version);
    // Only meaningful when another version is known to have worked here:
    // there is nothing to recover to on a fresh installation.
    let recovery = match &last_good {
        Some(good) if retry_of_pending && good != version && attempts >= 1 => Some(RecoveryInfo {
            failed_version: version.to_string(),
            last_good_version: Some(good.clone()),
            attempts,
        }),
        _ => None,
    };
    store.set_meta("launch_pending_version", version)?;
    store.set_meta(
        "launch_pending_attempts",
        &(if retry_of_pending { attempts + 1 } else { 1 }).to_string(),
    )?;

    let updated_from = last_app.clone().filter(|v| v != version);
    let mut history: Vec<UpdateRecord> = store
        .get_meta("update_history")?
        .and_then(|h| serde_json::from_str(&h).ok())
        .unwrap_or_default();
    if let Some(from) = &updated_from {
        log::info!("ComInspect updated from {from} to {version}");
        history.push(UpdateRecord {
            from: from.clone(),
            to: version.to_string(),
            at: now_ms(),
        });
        if history.len() > 20 {
            history.drain(..history.len() - 20);
        }
        store.set_meta(
            "update_history",
            &serde_json::to_string(&history).unwrap_or_default(),
        )?;
    }
    store.set_meta("last_app_version", version)?;
    let previous_version = history
        .iter()
        .rev()
        .find(|r| r.to == version)
        .map(|r| r.from.clone());

    Ok(StartupInfo {
        version: version.to_string(),
        updated_from,
        first_run: last_app.is_none(),
        recovery,
        previous_version,
    })
}

/// Called once the UI has rendered: this version launches successfully.
pub fn confirm_ready(store: &Store, version: &str) -> Result<()> {
    store.set_meta("last_good_version", version)?;
    store.delete_meta("launch_pending_version")?;
    store.delete_meta("launch_pending_attempts")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_recovery_banner_on_fresh_install() {
        let store = Store::open_in_memory().unwrap();
        record_launch(&store, "1.0.0").unwrap();
        // The first launch never reached the ready state.
        let second = record_launch(&store, "1.0.0").unwrap();
        assert!(second.recovery.is_none());
    }

    #[test]
    fn detects_updates_and_failed_launches() {
        let store = Store::open_in_memory().unwrap();
        let first = record_launch(&store, "1.0.0").unwrap();
        assert!(first.first_run);
        assert!(first.recovery.is_none());
        confirm_ready(&store, "1.0.0").unwrap();

        // Update to 1.1.0 which fails to reach the ready state twice.
        let s = record_launch(&store, "1.1.0").unwrap();
        assert_eq!(s.updated_from.as_deref(), Some("1.0.0"));
        assert_eq!(s.previous_version.as_deref(), Some("1.0.0"));
        assert!(s.recovery.is_none(), "first attempt is not a failure yet");
        let s = record_launch(&store, "1.1.0").unwrap();
        let recovery = s.recovery.unwrap();
        assert_eq!(recovery.last_good_version.as_deref(), Some("1.0.0"));
        assert_eq!(recovery.attempts, 1);
        assert!(s.updated_from.is_none());

        confirm_ready(&store, "1.1.0").unwrap();
        let s = record_launch(&store, "1.1.0").unwrap();
        assert!(s.recovery.is_none());
        assert_eq!(s.previous_version.as_deref(), Some("1.0.0"));
    }
}
