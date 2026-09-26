//! Persistent device database and inventory reconciliation.
//!
//! * [`Store`] owns the SQLite file: integrity checks, versioned migrations
//!   with automatic backups, forward-compatibility rules, settings and meta
//!   values.
//! * [`Inventory`] reconciles each scan with the database (identity matching,
//!   history, events) and builds the view model the UI renders.

mod error;
mod inventory;
pub mod schema;
mod store;
mod transfer;
pub mod view;

pub use error::{Result, StoreError};
pub use inventory::{Inventory, ReconcileOutcome};
pub use store::{
    BACKUP_RETENTION, BackupInfo, OpenReport, Store, backups_dir_for, list_backups, restore_backup,
};

#[cfg(test)]
mod tests;
