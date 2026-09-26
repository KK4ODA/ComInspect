//! Platform-independent core of ComInspect.
//!
//! * [`model`] — the normalized [`DiscoveredPort`](model::DiscoveredPort)
//!   every platform adapter produces.
//! * [`identity`] — fingerprint derivation and matching of ports to known
//!   devices.
//! * [`hints`] — the known-device hint database.
//! * [`analysis`] — conflict and health findings.
//! * [`user`] — user-assigned identity (nickname, purpose, CAT status …).
//! * [`export`] — the portable `serial-port-inventory.json` format.
//!
//! This crate performs no I/O and has no operating-system dependencies.

pub mod analysis;
pub mod export;
pub mod hints;
pub mod identity;
pub mod model;
pub mod time;
pub mod user;

pub use model::{DiscoveredPort, Presence, ScanResult, Transport};
