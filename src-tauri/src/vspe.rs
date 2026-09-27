//! The Eterlogic VSPE configuration shown in the app. It comes from VSPE's
//! startup configuration, a file the user chose, or the newest `.vspe` file
//! in a folder the user chose. Only the file is read; VSPE itself is never
//! touched.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use cominspect_core::vspe::VspeDevice;
use cominspect_platform::vspe;
use cominspect_store::Store;

/// Setting holding the [`VspeSource`].
const SETTING_SOURCE: &str = "vspe.source";
/// Setting written by 0.3.0: the path of a file the user chose.
const LEGACY_SETTING_FILE: &str = "vspe.file";

/// Where the VSPE configuration comes from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum VspeSource {
    /// VSPE's startup configuration.
    #[default]
    Autostart,
    /// A file the user chose.
    File { path: String },
    /// The newest `.vspe` file in a folder the user chose.
    Folder { path: String },
}

impl VspeSource {
    pub fn load(store: &Store) -> cominspect_store::Result<VspeSource> {
        if let Some(source) = store.get_setting::<VspeSource>(SETTING_SOURCE)? {
            return Ok(source);
        }
        Ok(match store.get_setting::<String>(LEGACY_SETTING_FILE)? {
            Some(path) => VspeSource::File { path },
            None => VspeSource::Autostart,
        })
    }

    pub fn save(&self, store: &Store) -> cominspect_store::Result<()> {
        store.set_setting(SETTING_SOURCE, self)?;
        store.delete_setting(LEGACY_SETTING_FILE)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VspeView {
    /// Where the configuration comes from, and the file or folder chosen.
    pub source: VspeSource,
    /// Where VSPE keeps its startup configuration (Windows only).
    pub autostart_path: Option<String>,
    /// The configuration file read, if any.
    pub file: Option<String>,
    pub devices: Vec<VspeDevice>,
    /// Why no configuration could be read.
    pub error: Option<String>,
    /// When the file read was last saved (ms since the epoch).
    pub modified_at: Option<i64>,
}

/// Reads the configuration `source` points at.
pub fn view(source: VspeSource) -> VspeView {
    let (path, missing) = match &source {
        VspeSource::Autostart => (vspe::autostart_config(), None),
        VspeSource::File { path } => (Some(PathBuf::from(path)), None),
        VspeSource::Folder { path } => (
            vspe::newest_config_in(Path::new(path)),
            Some(format!(
                "There are no VSPE configuration files (.vspe) in {path}."
            )),
        ),
    };
    let mut view = VspeView {
        autostart_path: vspe::autostart_path().map(|p| p.display().to_string()),
        source,
        file: None,
        devices: Vec::new(),
        error: None,
        modified_at: None,
    };
    let Some(path) = path else {
        view.error = missing;
        return view;
    };
    match vspe::read_config(&path) {
        Ok(devices) => view.devices = devices,
        Err(e) => view.error = Some(e),
    }
    view.file = Some(path.display().to_string());
    view.modified_at = modified_at(&path);
    view
}

fn modified_at(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_file_chosen_in_0_3_0() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(VspeSource::load(&store).unwrap(), VspeSource::Autostart);
        store
            .set_setting(LEGACY_SETTING_FILE, &"C:\\VSPE\\shack.vspe")
            .unwrap();
        let legacy = VspeSource::load(&store).unwrap();
        assert_eq!(
            legacy,
            VspeSource::File {
                path: "C:\\VSPE\\shack.vspe".into()
            }
        );
        let folder = VspeSource::Folder {
            path: "C:\\VSPE".into(),
        };
        folder.save(&store).unwrap();
        assert_eq!(VspeSource::load(&store).unwrap(), folder);
        assert_eq!(
            store.get_setting::<String>(LEGACY_SETTING_FILE).unwrap(),
            None
        );
    }

    #[test]
    fn reads_the_newest_file_in_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = VspeSource::Folder {
            path: dir.path().display().to_string(),
        };
        let empty = view(folder.clone());
        assert!(empty.error.unwrap().contains("no VSPE configuration files"));
        let mut bytes = vec![0, 0, 0, 0, 0x44, 0x33, 0x22, 0x11];
        for text in ["Pair", "21;22;0"] {
            bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
        std::fs::write(dir.path().join("shack.vspe"), bytes).unwrap();
        let found = view(folder);
        assert_eq!(found.error, None);
        assert!(found.file.unwrap().ends_with("shack.vspe"));
        assert_eq!(found.devices.len(), 1);
        assert!(found.modified_at.is_some());
    }
}
