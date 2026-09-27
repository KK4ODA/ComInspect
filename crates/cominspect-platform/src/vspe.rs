//! Where Eterlogic VSPE keeps its configuration, and reading it (see
//! [`cominspect_core::vspe`]). VSPE is never started, stopped or asked
//! anything: only its configuration file is read.

use std::path::{Path, PathBuf};

use cominspect_core::vspe::{self, VspeDevice};

/// Files larger than this are not VSPE configurations.
const MAX_FILE_BYTES: u64 = 4 << 20;

/// Where VSPE keeps its startup configuration, which its service loads when
/// Windows starts (VSPE 1.5 and later: File → Save as autostart config),
/// whether or not the file exists.
pub fn autostart_path() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let base = std::env::var_os("ProgramData")?;
    Some(
        PathBuf::from(base)
            .join("Eterlogic")
            .join("VSPE")
            .join("autostart.vspe"),
    )
}

/// VSPE's startup configuration, when it exists.
pub fn autostart_config() -> Option<PathBuf> {
    autostart_path().filter(|p| p.is_file())
}

/// The most recently changed `.vspe` file directly in `folder`, for people
/// who save a new configuration file for every change.
pub fn newest_config_in(folder: &Path) -> Option<PathBuf> {
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("vspe"))
        })
        .filter_map(|path| {
            let meta = std::fs::metadata(&path).ok()?;
            meta.is_file().then(|| (meta.modified().ok(), path))
        })
        .max()
        .map(|(_, path)| path)
}

/// Reads the devices from a `.vspe` file.
pub fn read_config(path: &Path) -> Result<Vec<VspeDevice>, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let meta = std::fs::metadata(path).map_err(|e| fail(&e))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(fail(&"too large to be a VSPE configuration"));
    }
    let bytes = std::fs::read(path).map_err(|e| fail(&e))?;
    vspe::parse_config(&bytes).map_err(|e| fail(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_files_and_explains_failures() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("shack.vspe");
        let mut bytes = vec![0, 0, 0, 0, 0x44, 0x33, 0x22, 0x11];
        for text in ["Pair", "21;22;0"] {
            bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
        std::fs::write(&good, bytes).unwrap();
        let devices = read_config(&good).unwrap();
        assert_eq!(devices[0].describe(), "Pair: COM21 ↔ COM22");

        let newest = |dir: &Path| newest_config_in(dir).map(|p| p.file_name().unwrap().to_owned());
        assert_eq!(newest(dir.path()).as_deref(), Some("shack.vspe".as_ref()));

        let other = dir.path().join("notes.txt");
        std::fs::write(&other, "hello").unwrap();
        assert!(
            read_config(&other)
                .unwrap_err()
                .contains("not a VSPE configuration")
        );
        assert!(read_config(&dir.path().join("missing.vspe")).is_err());
    }

    #[test]
    fn picks_the_newest_configuration_in_a_folder() {
        use std::time::{Duration, SystemTime};
        let dir = tempfile::tempdir().unwrap();
        let day = Duration::from_secs(86_400);
        let now = SystemTime::now();
        for (name, age) in [
            ("Lenovo_X1_092026.vspe", 7),
            ("Lenovo_X1_092726.VSPE", 0),
            ("notes.txt", 0),
            ("old.vspe", 30),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, b"").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(now - day * age).unwrap();
        }
        std::fs::create_dir(dir.path().join("folder.vspe")).unwrap();
        let newest = newest_config_in(dir.path()).unwrap();
        assert_eq!(newest.file_name().unwrap(), "Lenovo_X1_092726.VSPE");
        assert_eq!(newest_config_in(&dir.path().join("missing")), None);
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(newest_config_in(empty.path()), None);
    }
}
