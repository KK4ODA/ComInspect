//! View model sent to the UI. Field names are camelCase on the wire; the
//! TypeScript mirror lives in `ui/src/lib/types.ts`.

use serde::{Deserialize, Serialize};

use cominspect_core::analysis::{Finding, Severity};
use cominspect_core::hints::Hint;
use cominspect_core::identity::MatchBasis;
use cominspect_core::model::{DiscoveredPort, PlatformCapabilities, ReservedPorts, Transport};
use cominspect_core::user::{CatStatus, Category, Purpose};

/// Connection state of a device in the inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortStatus {
    /// Attached and working.
    Connected,
    /// Attached but the driver reports a problem.
    Problem,
    /// Not attached; the OS still remembers the port (Windows hidden device).
    AbsentOs,
    /// Not attached; known only from ComInspect's history.
    Absent,
    /// Imported from another computer and not yet seen on this one.
    Awaiting,
}

impl PortStatus {
    pub fn is_connected(self) -> bool {
        matches!(self, PortStatus::Connected | PortStatus::Problem)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastSeenSource {
    /// Recorded by ComInspect.
    App,
    /// Reported by the operating system (Windows device history).
    Os,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortRow {
    pub device_id: i64,
    pub uuid: String,
    pub status: PortStatus,
    /// Current port, or the last known one.
    pub port: Option<String>,
    pub port_short: Option<String>,
    /// Most recent different port this device used.
    pub previous_port: Option<String>,
    /// When the device was first seen on its current port, if it has used
    /// another port before.
    pub port_changed_at: Option<i64>,
    pub nickname: Option<String>,
    pub equipment: Option<String>,
    pub category: Option<Category>,
    pub purpose: Option<Purpose>,
    pub cat_status: CatStatus,
    pub notes: Option<String>,
    pub ignored: bool,
    pub device_label: String,
    /// Short label from the hint database ("Enhanced COM port").
    pub hint_label: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub transport: Transport,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub serial_number: Option<String>,
    pub interface_number: Option<u8>,
    pub bt_address: Option<String>,
    pub virtual_provider: Option<String>,
    pub first_seen: i64,
    pub last_seen: Option<i64>,
    pub last_seen_source: Option<LastSeenSource>,
    /// Highest severity among warnings/errors concerning this device.
    pub severity: Option<Severity>,
    pub finding_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewFinding {
    #[serde(flatten)]
    pub finding: Finding,
    pub device_ids: Vec<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub total: usize,
    pub connected: usize,
    pub disconnected: usize,
    pub problems: usize,
    pub warnings: usize,
    pub ignored: usize,
    pub awaiting: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanMeta {
    pub platform: String,
    pub scanned_at: i64,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
    pub capabilities: PlatformCapabilities,
    pub reserved: Option<ReservedPorts>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    /// The database is temporary (changes are not saved).
    pub temporary: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryView {
    pub rows: Vec<PortRow>,
    pub findings: Vec<ViewFinding>,
    pub summary: Summary,
    pub scan: Option<ScanMeta>,
    pub storage: StorageInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRecord {
    pub kind: String,
    pub value: String,
    pub strength: u8,
    pub portable: bool,
    pub description: String,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortHistoryEntry {
    pub port: String,
    pub first_observed: i64,
    pub last_observed: i64,
    pub last_connected: Option<i64>,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    pub id: i64,
    pub ts: i64,
    pub device_id: Option<i64>,
    pub kind: String,
    pub port: Option<String>,
    pub previous_port: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeCandidate {
    pub device_id: i64,
    pub label: String,
    pub port: Option<String>,
    pub status: PortStatus,
    pub last_seen: Option<i64>,
    pub reason: String,
}

/// Machine-specific data carried over from an import (informational).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFrom {
    pub hostname: Option<String>,
    pub os: String,
    pub last_port: Option<String>,
    pub port_history: Vec<String>,
    pub imported_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDetail {
    pub row: PortRow,
    /// Everything known about the hardware: live data when present,
    /// otherwise the last stored observation.
    pub snapshot: Option<DiscoveredPort>,
    pub snapshot_live: bool,
    pub recognized_by: Option<MatchBasis>,
    /// Plain-language explanation of how the device is recognized.
    pub identity_note: String,
    pub keys: Vec<KeyRecord>,
    pub port_history: Vec<PortHistoryEntry>,
    pub events: Vec<EventRecord>,
    pub hints: Vec<Hint>,
    pub findings: Vec<ViewFinding>,
    pub merge_candidates: Vec<MergeCandidate>,
    pub imported_from: Option<ImportedFrom>,
}

/// Something that happened to a device during a reconcile pass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEvent {
    pub kind: InventoryEventKind,
    pub device_id: i64,
    pub label: String,
    pub port: Option<String>,
    pub previous_port: Option<String>,
    pub ts: i64,
    /// Detected during the first scan after startup (not a live change).
    pub initial: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryEventKind {
    NewDevice,
    Connected,
    Disconnected,
    PortChanged,
}

impl InventoryEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryEventKind::NewDevice => "new_device",
            InventoryEventKind::Connected => "connected",
            InventoryEventKind::Disconnected => "disconnected",
            InventoryEventKind::PortChanged => "port_changed",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub matched: usize,
    pub created: usize,
    pub unchanged: usize,
    pub same_machine: bool,
    pub source_hostname: Option<String>,
    pub source_os: String,
}
