//! Operating-system serial port discovery, monitoring and diagnostics.
//!
//! Every adapter produces the normalized [`ScanResult`] defined in
//! `cominspect-core`. Each adapter keeps raw data collection (FFI, sysfs)
//! separate from pure interpretation functions, which are compiled and
//! unit-tested on every platform.

pub mod cat;
pub mod diagnostics;
pub mod launch;
pub mod monitor;
pub mod usage;
pub mod vspe;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
use linux as os;

pub mod windows;
#[cfg(windows)]
use windows as os;

pub mod macos;
#[cfg(target_os = "macos")]
use macos as os;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod other;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
use other as os;

use cominspect_core::model::ScanResult;

/// Enumerates all serial ports known to the operating system. Never panics;
/// problems are reported in [`ScanResult::warnings`].
pub fn discover() -> ScanResult {
    let result = std::panic::catch_unwind(os::discover);
    match result {
        Ok(scan) => scan,
        Err(_) => ScanResult {
            platform: platform_name().into(),
            scanned_at: cominspect_core::time::now_ms(),
            warnings: vec!["serial port enumeration failed unexpectedly".into()],
            capabilities: os::capabilities(),
            ..Default::default()
        },
    }
}

/// `windows`, `linux`, `macos` or `other`.
pub fn platform_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
}
