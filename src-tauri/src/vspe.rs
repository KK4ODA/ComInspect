//! The Eterlogic VSPE configuration shown in the app: VSPE's startup
//! configuration, or a `.vspe` file the user chose. Only the file is read;
//! VSPE itself is never touched.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use cominspect_core::vspe::VspeDevice;
use cominspect_platform::vspe;

/// Setting holding the path of a `.vspe` file the user chose.
pub const SETTING_FILE: &str = "vspe.file";

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VspeView {
    /// The configuration file read, if any.
    pub file: Option<String>,
    /// True when the user chose the file; false for VSPE's startup
    /// configuration.
    pub chosen: bool,
    pub devices: Vec<VspeDevice>,
    /// Why the file could not be read.
    pub error: Option<String>,
    /// When the file last changed (ms since the epoch).
    pub modified_at: Option<i64>,
}

/// Reads the chosen file, or VSPE's startup configuration.
pub fn view(chosen: Option<String>) -> VspeView {
    let (path, is_chosen) = match chosen {
        Some(file) => (Some(PathBuf::from(file)), true),
        None => (vspe::autostart_config(), false),
    };
    let Some(path) = path else {
        return VspeView::default();
    };
    let (devices, error) = match vspe::read_config(&path) {
        Ok(devices) => (devices, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    VspeView {
        file: Some(path.display().to_string()),
        chosen: is_chosen,
        devices,
        error,
        modified_at: modified_at(&path),
    }
}

fn modified_at(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}
