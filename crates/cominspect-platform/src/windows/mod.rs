//! Windows adapter.
//!
//! * `interpret` — pure interpretation of raw device data (compiled and
//!   tested on every platform).
//! * `collect` — Configuration Manager / registry / msports FFI (Windows only).
//! * `watch` — Plug and Play and serial-device-map notifications (Windows only).
#![cfg_attr(not(windows), allow(dead_code))]

pub mod interpret;

#[cfg(windows)]
mod collect;
#[cfg(windows)]
mod watch;

pub use interpret::capabilities;

#[cfg(windows)]
pub use collect::discover;
#[cfg(windows)]
pub(crate) use watch::start_sources;
