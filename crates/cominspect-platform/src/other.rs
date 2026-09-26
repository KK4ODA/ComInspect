//! Fallback for operating systems without an adapter.

use std::sync::mpsc::Sender;

use cominspect_core::model::{PlatformCapabilities, ScanResult};

use crate::monitor::{ChangeHint, HintSource};

pub fn discover() -> ScanResult {
    ScanResult {
        platform: "other".into(),
        scanned_at: cominspect_core::time::now_ms(),
        warnings: vec!["serial port discovery is not supported on this operating system".into()],
        capabilities: capabilities(),
        ..Default::default()
    }
}

pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        monitoring: "Polling".into(),
        ..Default::default()
    }
}

pub(crate) fn start_sources(_tx: Sender<ChangeHint>) -> Vec<Box<dyn HintSource>> {
    Vec::new()
}
