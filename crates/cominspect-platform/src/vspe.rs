//! Where Eterlogic VSPE keeps its configuration, and reading it (see
//! [`cominspect_core::vspe`]). VSPE is never started, stopped or asked
//! anything: only its configuration file is read.

use std::path::{Path, PathBuf};

use cominspect_core::vspe::{self, VspeDevice};

/// Files larger than this are not VSPE configurations.
const MAX_FILE_BYTES: u64 = 4 << 20;

/// VSPE's startup configuration, which its service loads when Windows starts
/// (VSPE 1.5 and later: File → Save as autostart config).
pub fn autostart_config() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let base = std::env::var_os("ProgramData")?;
    let path = PathBuf::from(base)
        .join("Eterlogic")
        .join("VSPE")
        .join("autostart.vspe");
    path.is_file().then_some(path)
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

        let other = dir.path().join("notes.txt");
        std::fs::write(&other, "hello").unwrap();
        assert!(
            read_config(&other)
                .unwrap_err()
                .contains("not a VSPE configuration")
        );
        assert!(read_config(&dir.path().join("missing.vspe")).is_err());
    }
}
