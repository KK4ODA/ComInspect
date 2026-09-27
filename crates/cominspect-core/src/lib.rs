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
//! * [`usage`] — which programs have a port open.
//! * [`vspe`] — Eterlogic VSPE configuration files (virtual port links).
//!
//! This crate performs no I/O and has no operating-system dependencies.

pub mod analysis;
pub mod export;
pub mod hints;
pub mod identity;
pub mod model;
pub mod time;
pub mod usage;
pub mod user;
pub mod vspe;

pub use model::{DiscoveredPort, Presence, ScanResult, Transport};
