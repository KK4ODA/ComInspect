//! Inventory service: reconciles scans with the device database and builds
//! the view model for the UI.

use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, Row, Transaction, params};

use cominspect_core::analysis::{self, Finding, Severity};
use cominspect_core::hints::KnownDeviceDb;
use cominspect_core::identity::{
    self, DeviceAttrs, IdentityKey, KeyKind, KnownDevice, KnownKey, MatchBasis,
    normalize_bt_address, normalize_serial, usable_serial,
};
use cominspect_core::model::{
    DiscoveredPort, ScanResult, Transport, short_port_name, strip_port_suffix,
};
use cominspect_core::time::now_ms;
use cominspect_core::user::{CatStatus, Category, IdentityPatch, Purpose, UserIdentity};

use crate::error::{Result, StoreError};
use crate::store::Store;
use crate::view::*;

/// Events older than this many rows are pruned.
const EVENT_RETENTION: i64 = 5000;
/// Minimum interval between "still here" writes for an unchanged device.
const TOUCH_INTERVAL_MS: i64 = 60_000;
/// Minimum interval between writes for an unchanged hidden (absent) device.
const ABSENT_TOUCH_INTERVAL_MS: i64 = 3_600_000;

/// A device row as stored in the database.
#[derive(Clone, Debug)]
pub(crate) struct DeviceRecord {
    pub id: i64,
    pub uuid: String,
    pub identity: UserIdentity,
    pub ignored: bool,
    pub transport: Transport,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub serial_number: Option<String>,
    pub interface_number: Option<u8>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub description: Option<String>,
    pub bt_address: Option<String>,
    pub virtual_provider: Option<String>,
    pub snapshot_json: Option<String>,
    pub snapshot_presence: Option<String>,
    pub snapshot_hash: Option<String>,
    pub last_port: Option<String>,
    pub connected: bool,
    pub first_seen: i64,
    pub last_seen: Option<i64>,
    pub last_observed: i64,
    pub awaiting: bool,
    pub imported_from: Option<String>,
}

pub(crate) const DEVICE_COLUMNS: &str = "id, uuid, nickname, equipment, category, purpose, \
    cat_status, notes, ignored, transport, vid, pid, serial_number, interface_number, \
    manufacturer, product, description, bt_address, virtual_provider, last_snapshot, \
    snapshot_presence, snapshot_hash, last_port, connected, first_seen, last_seen, \
    last_observed, awaiting, imported_from";

pub(crate) fn read_device(row: &Row<'_>) -> rusqlite::Result<DeviceRecord> {
    let category: Option<String> = row.get(4)?;
    let purpose: Option<String> = row.get(5)?;
    let cat_status: String = row.get(6)?;
    let transport: String = row.get(9)?;
    Ok(DeviceRecord {
        id: row.get(0)?,
        uuid: row.get(1)?,
        identity: UserIdentity {
            nickname: row.get(2)?,
            equipment: row.get(3)?,
            category: category.as_deref().and_then(Category::parse),
            purpose: purpose.as_deref().and_then(Purpose::parse),
            cat_status: CatStatus::parse(&cat_status).unwrap_or_default(),
            notes: row.get(7)?,
        },
        ignored: row.get::<_, i64>(8)? != 0,
        transport: Transport::parse(&transport).unwrap_or_default(),
        vid: row.get::<_, Option<i64>>(10)?.map(|v| v as u16),
        pid: row.get::<_, Option<i64>>(11)?.map(|v| v as u16),
        serial_number: row.get(12)?,
        interface_number: row.get::<_, Option<i64>>(13)?.map(|v| v as u8),
        manufacturer: row.get(14)?,
        product: row.get(15)?,
        description: row.get(16)?,
        bt_address: row.get(17)?,
        virtual_provider: row.get(18)?,
        snapshot_json: row.get(19)?,
        snapshot_presence: row.get(20)?,
        snapshot_hash: row.get(21)?,
        last_port: row.get(22)?,
        connected: row.get::<_, i64>(23)? != 0,
        first_seen: row.get(24)?,
        last_seen: row.get(25)?,
        last_observed: row.get(26)?,
        awaiting: row.get::<_, i64>(27)? != 0,
        imported_from: row.get(28)?,
    })
}

impl DeviceRecord {
    pub(crate) fn attrs(&self) -> DeviceAttrs {
        DeviceAttrs {
            transport: self.transport,
            vid: self.vid,
            pid: self.pid,
            serial: self
                .serial_number
                .as_deref()
                .map(normalize_serial)
                .filter(|s| !s.is_empty()),
            interface: self.interface_number,
            bt_address: self.bt_address.as_deref().and_then(normalize_bt_address),
        }
    }

    fn snapshot(&self) -> Option<DiscoveredPort> {
        self.snapshot_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
    }
}

/// Summary columns derived from a port.
struct HwSummary {
    transport: Transport,
    vid: Option<u16>,
    pid: Option<u16>,
    serial: Option<String>,
    interface: Option<u8>,
    manufacturer: Option<String>,
    product: Option<String>,
    description: Option<String>,
    bt_address: Option<String>,
    virtual_provider: Option<String>,
}

impl HwSummary {
    fn of(port: &DiscoveredPort) -> HwSummary {
        let usb = port.usb.as_ref();
        HwSummary {
            transport: port.transport,
            vid: usb.map(|u| u.vid),
            pid: usb.map(|u| u.pid),
            serial: usb
                .and_then(|u| u.serial_number.clone())
                .filter(|s| !s.trim().is_empty()),
            interface: usb.and_then(|u| u.interface_number),
            manufacturer: port
                .manufacturer
                .clone()
                .or_else(|| usb.and_then(|u| u.manufacturer.clone())),
            product: usb.and_then(|u| u.product.clone()),
            description: port
                .description
                .as_deref()
                .or(port.friendly_name.as_deref())
                .map(strip_port_suffix)
                .filter(|d| !d.is_empty()),
            bt_address: port
                .bluetooth
                .as_ref()
                .and_then(|b| b.address.as_deref())
                .and_then(normalize_bt_address),
            virtual_provider: port.virtual_port.as_ref().map(|v| v.provider.clone()),
        }
    }
}

/// 64-bit FNV-1a, used to detect snapshot changes without comparing JSON.
fn fnv_hex(data: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Combines a live observation of an absent (hidden) device with the richer
/// data recorded while it was connected.
fn enrich(live: &DiscoveredPort, stored: Option<&DiscoveredPort>) -> DiscoveredPort {
    let mut out = live.clone();
    let Some(stored) = stored.filter(|s| s.is_present()) else {
        return out;
    };
    if live.is_present() {
        return out;
    }
    match (&mut out.usb, &stored.usb) {
        (None, Some(s)) => out.usb = Some(s.clone()),
        (Some(u), Some(s)) if u.vid == s.vid && u.pid == s.pid => {
            u.serial_number = u.serial_number.take().or_else(|| s.serial_number.clone());
            u.interface_number = u.interface_number.or(s.interface_number);
            u.interface_name = u.interface_name.take().or_else(|| s.interface_name.clone());
            u.manufacturer = u.manufacturer.take().or_else(|| s.manufacturer.clone());
            u.product = u.product.take().or_else(|| s.product.clone());
            u.revision = u.revision.or(s.revision);
            u.location = u.location.take().or_else(|| s.location.clone());
            u.location_label = u.location_label.take().or_else(|| s.location_label.clone());
            u.device_node = u.device_node.take().or_else(|| s.device_node.clone());
        }
        _ => {}
    }
    if out.bluetooth.is_none() {
        out.bluetooth.clone_from(&stored.bluetooth);
    }
    if out.manufacturer.is_none() {
        out.manufacturer.clone_from(&stored.manufacturer);
    }
    if out.description.is_none() {
        out.description.clone_from(&stored.description);
    }
    let sys = &mut out.system;
    let ss = &stored.system;
    macro_rules! fill {
        ($($f:ident),*) => { $( if sys.$f.is_none() { sys.$f.clone_from(&ss.$f); } )* };
    }
    fill!(
        parent_instance_id,
        container_id,
        driver,
        driver_provider,
        driver_version,
        driver_date,
        driver_inf,
        location_info
    );
    if sys.location_paths.is_empty() {
        sys.location_paths.clone_from(&ss.location_paths);
    }
    out
}

/// Outcome of a reconcile pass.
#[derive(Clone, Debug, Default)]
pub struct ReconcileOutcome {
    pub events: Vec<InventoryEvent>,
}

pub struct Inventory {
    pub(crate) store: Store,
    platform: String,
    hints: &'static KnownDeviceDb,
    last_scan: Option<ScanResult>,
    /// Device ID → index into `last_scan.ports`.
    scan_index: HashMap<i64, usize>,
    basis: HashMap<i64, MatchBasis>,
    findings: Vec<Finding>,
    initialized: bool,
}

fn record_label(identity: &UserIdentity, fallback: &str) -> String {
    identity
        .nickname
        .clone()
        .unwrap_or_else(|| fallback.to_string())
}

impl Inventory {
    pub fn new(store: Store, platform: &str) -> Inventory {
        Inventory {
            store,
            platform: platform.to_string(),
            hints: KnownDeviceDb::bundled(),
            last_scan: None,
            scan_index: HashMap::new(),
            basis: HashMap::new(),
            findings: Vec::new(),
            initialized: false,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    pub fn into_store(self) -> Store {
        self.store
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }

    pub fn last_scan(&self) -> Option<&ScanResult> {
        self.last_scan.as_ref()
    }

    pub(crate) fn load_devices(&self) -> Result<Vec<DeviceRecord>> {
        let mut stmt = self
            .store
            .conn
            .prepare(&format!("SELECT {DEVICE_COLUMNS} FROM devices ORDER BY id"))?;
        let rows = stmt.query_map([], read_device)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn load_device(&self, id: i64) -> Result<DeviceRecord> {
        self.store
            .conn
            .query_row(
                &format!("SELECT {DEVICE_COLUMNS} FROM devices WHERE id = ?1"),
                params![id],
                read_device,
            )
            .optional()?
            .ok_or(StoreError::NotFound(id))
    }

    fn load_keys(&self) -> Result<HashMap<i64, Vec<KnownKey>>> {
        let mut stmt = self
            .store
            .conn
            .prepare("SELECT device_id, kind, value, strength, last_seen FROM identity_keys")?;
        let mut out: HashMap<i64, Vec<KnownKey>> = HashMap::new();
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        for row in rows {
            let (device, kind, value, strength, last_seen) = row?;
            let Some(kind) = KeyKind::parse(&kind) else {
                continue;
            };
            out.entry(device).or_default().push(KnownKey {
                key: IdentityKey::new(kind, value, strength.clamp(0, 100) as u8),
                last_seen,
            });
        }
        Ok(out)
    }

    fn load_assignments(&self) -> Result<HashMap<i64, Vec<PortHistoryEntry>>> {
        let mut stmt = self.store.conn.prepare(
            "SELECT device_id, port_name, first_observed, last_observed, last_connected
             FROM port_assignments ORDER BY device_id, last_observed DESC, first_observed DESC",
        )?;
        let mut out: HashMap<i64, Vec<PortHistoryEntry>> = HashMap::new();
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                PortHistoryEntry {
                    port: r.get(1)?,
                    first_observed: r.get(2)?,
                    last_observed: r.get(3)?,
                    last_connected: r.get(4)?,
                    current: false,
                },
            ))
        })?;
        for row in rows {
            let (device, entry) = row?;
            out.entry(device).or_default().push(entry);
        }
        Ok(out)
    }

    /// Reconciles a scan with the database: matches ports to known devices,
    /// records new devices, history and events.
    pub fn reconcile(&mut self, scan: ScanResult) -> Result<ReconcileOutcome> {
        let now = scan.scanned_at;
        let devices = self.load_devices()?;
        let mut keys = self.load_keys()?;
        let known: Vec<KnownDevice> = devices
            .iter()
            .map(|d| KnownDevice {
                id: d.id,
                keys: keys.remove(&d.id).unwrap_or_default(),
                attrs: d.attrs(),
                last_port: d.last_port.clone(),
                last_observed: d.last_observed,
            })
            .collect();
        let matches = identity::match_ports(&scan.ports, &known);
        let records: HashMap<i64, &DeviceRecord> = devices.iter().map(|d| (d.id, d)).collect();
        let initial = !self.initialized;

        let mut events: Vec<InventoryEvent> = Vec::new();
        let mut scan_index = HashMap::new();
        let mut basis = HashMap::new();
        let tx = self.store.conn.transaction()?;

        for (i, (port, m)) in scan.ports.iter().zip(&matches).enumerate() {
            let present = port.is_present();
            let json = serde_json::to_string(port)?;
            let hash = fnv_hex(&json);
            let summary = HwSummary::of(port);
            let device_id = match m.device_id {
                Some(id) => {
                    let rec = records[&id];
                    if let Some(b) = m.basis {
                        basis.insert(id, b);
                    }
                    let label = record_label(&rec.identity, &port.device_label());
                    let moved = rec.last_port.as_deref() != Some(port.port_name.as_str());
                    if present && !rec.connected {
                        events.push(InventoryEvent {
                            kind: InventoryEventKind::Connected,
                            device_id: id,
                            label,
                            port: Some(port.port_name.clone()),
                            previous_port: rec.last_port.clone().filter(|_| moved),
                            ts: now,
                            initial,
                        });
                    } else if present && moved {
                        events.push(InventoryEvent {
                            kind: InventoryEventKind::PortChanged,
                            device_id: id,
                            label,
                            port: Some(port.port_name.clone()),
                            previous_port: rec.last_port.clone(),
                            ts: now,
                            initial,
                        });
                    } else if !present && rec.connected {
                        events.push(InventoryEvent {
                            kind: InventoryEventKind::Disconnected,
                            device_id: id,
                            label,
                            port: Some(port.port_name.clone()),
                            previous_port: None,
                            ts: now,
                            initial,
                        });
                    }
                    update_observed(&tx, rec, port, present, &json, &hash, &summary, now)?;
                    id
                }
                None => {
                    let id = insert_device(&tx, port, present, &json, &hash, &summary, now)?;
                    events.push(InventoryEvent {
                        kind: InventoryEventKind::NewDevice,
                        device_id: id,
                        label: port.device_label(),
                        port: Some(port.port_name.clone()),
                        previous_port: None,
                        ts: now,
                        initial,
                    });
                    id
                }
            };
            scan_index.insert(device_id, i);
            upsert_keys(&tx, device_id, &m.keys, now)?;
            upsert_assignment(&tx, device_id, &port.port_name, present, now)?;
        }

        for rec in &devices {
            if rec.connected && !scan_index.contains_key(&rec.id) {
                tx.execute(
                    "UPDATE devices SET connected = 0, updated_at = ?2 WHERE id = ?1",
                    params![rec.id, now],
                )?;
                let fallback = rec
                    .description
                    .clone()
                    .or_else(|| rec.last_port.clone())
                    .unwrap_or_default();
                events.push(InventoryEvent {
                    kind: InventoryEventKind::Disconnected,
                    device_id: rec.id,
                    label: record_label(&rec.identity, &fallback),
                    port: rec.last_port.clone(),
                    previous_port: None,
                    ts: now,
                    initial,
                });
            }
        }

        for e in &events {
            let detail = e.initial.then_some("detected at startup");
            tx.execute(
                "INSERT INTO events(ts, device_id, kind, port_name, previous_port, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    e.ts,
                    e.device_id,
                    e.kind.as_str(),
                    e.port,
                    e.previous_port,
                    detail
                ],
            )?;
        }
        if !events.is_empty() {
            tx.execute(
                "DELETE FROM events WHERE id <= (SELECT id FROM events ORDER BY id DESC LIMIT 1 OFFSET ?1)",
                params![EVENT_RETENTION],
            )?;
        }
        tx.commit()?;

        self.findings = analysis::analyze_scan(&scan);
        self.scan_index = scan_index;
        self.basis = basis;
        self.platform.clone_from(&scan.platform);
        self.last_scan = Some(scan);
        self.initialized = true;
        Ok(ReconcileOutcome { events })
    }

    /// Findings of the last scan, mapped to device IDs.
    fn view_findings(&self) -> Vec<ViewFinding> {
        let reverse: HashMap<usize, i64> = self.scan_index.iter().map(|(d, i)| (*i, *d)).collect();
        self.findings
            .iter()
            .map(|f| ViewFinding {
                finding: f.clone(),
                device_ids: f
                    .ports
                    .iter()
                    .filter_map(|i| reverse.get(i).copied())
                    .collect(),
            })
            .collect()
    }

    fn live_port(&self, device_id: i64) -> Option<&DiscoveredPort> {
        let index = *self.scan_index.get(&device_id)?;
        self.last_scan.as_ref()?.ports.get(index)
    }

    fn build_row(
        &self,
        d: &DeviceRecord,
        history: Option<&Vec<PortHistoryEntry>>,
        findings: &[ViewFinding],
    ) -> (PortRow, Option<DiscoveredPort>) {
        let live = self.live_port(d.id);
        let stored = d.snapshot();
        let snapshot = match live {
            Some(l) => Some(enrich(l, stored.as_ref())),
            None => stored,
        };
        let status = match live {
            Some(p) if p.is_present() && p.system.problem.is_some() => PortStatus::Problem,
            Some(p) if p.is_present() => PortStatus::Connected,
            Some(_) => PortStatus::AbsentOs,
            None if d.awaiting => PortStatus::Awaiting,
            None => PortStatus::Absent,
        };
        let port = live
            .map(|p| p.port_name.clone())
            .or_else(|| d.last_port.clone());

        let chip = match (d.vid, d.pid) {
            (Some(v), Some(p)) => self.hints.chip_for(v, p).map(str::to_string),
            _ => None,
        };
        let device_label = snapshot
            .as_ref()
            .map(|s| s.device_label())
            .filter(|label| Some(label.as_str()) != port.as_deref().map(short_port_name))
            .or_else(|| d.description.clone())
            .or_else(|| d.product.clone())
            .or(chip)
            .unwrap_or_else(|| match d.transport {
                Transport::Unknown => "Serial port".to_string(),
                t => format!("{} serial port", t.label()),
            });
        let hint_label = snapshot.as_ref().and_then(|s| {
            self.hints
                .hints_for(s, &self.platform)
                .into_iter()
                .find_map(|h| h.short_label)
        });

        let (previous_port, port_changed_at) = match (history, port.as_deref()) {
            (Some(entries), Some(current)) => {
                let previous = entries
                    .iter()
                    .find(|e| !e.port.eq_ignore_ascii_case(current))
                    .map(|e| e.port.clone());
                let changed_at = previous.as_ref().and_then(|_| {
                    entries
                        .iter()
                        .find(|e| e.port.eq_ignore_ascii_case(current))
                        .map(|e| e.first_observed)
                });
                (previous, changed_at)
            }
            _ => (None, None),
        };

        let (last_seen, last_seen_source) = match d.last_seen {
            Some(ts) => (Some(ts), Some(LastSeenSource::App)),
            None => match snapshot
                .as_ref()
                .and_then(|s| s.os_times.last_seen_estimate())
            {
                Some(ts) => (Some(ts), Some(LastSeenSource::Os)),
                None => (None, None),
            },
        };

        let relevant: Vec<&ViewFinding> = findings
            .iter()
            .filter(|f| f.device_ids.contains(&d.id))
            .collect();
        let severity = relevant
            .iter()
            .map(|f| f.finding.severity)
            .filter(|s| *s >= Severity::Warning)
            .max();

        let usb = snapshot.as_ref().and_then(|s| s.usb.as_ref());
        let row = PortRow {
            device_id: d.id,
            uuid: d.uuid.clone(),
            status,
            port_short: port.as_deref().map(|p| short_port_name(p).to_string()),
            port,
            previous_port,
            port_changed_at,
            nickname: d.identity.nickname.clone(),
            equipment: d.identity.equipment.clone(),
            category: d.identity.category,
            purpose: d.identity.purpose,
            cat_status: d.identity.cat_status,
            notes: d.identity.notes.clone(),
            ignored: d.ignored,
            device_label,
            hint_label,
            manufacturer: d
                .manufacturer
                .clone()
                .or_else(|| snapshot.as_ref().and_then(|s| s.manufacturer.clone())),
            product: d
                .product
                .clone()
                .or_else(|| usb.and_then(|u| u.product.clone())),
            transport: d.transport,
            vid: d.vid.or(usb.map(|u| u.vid)),
            pid: d.pid.or(usb.map(|u| u.pid)),
            serial_number: d
                .serial_number
                .clone()
                .or_else(|| usb.and_then(|u| u.serial_number.clone())),
            interface_number: d.interface_number.or(usb.and_then(|u| u.interface_number)),
            bt_address: d.bt_address.clone(),
            virtual_provider: d.virtual_provider.clone(),
            first_seen: d.first_seen,
            last_seen,
            last_seen_source,
            severity,
            finding_count: relevant.len(),
        };
        (row, snapshot)
    }

    /// Builds the complete view for the UI.
    pub fn view(&self) -> Result<InventoryView> {
        let devices = self.load_devices()?;
        let assignments = self.load_assignments()?;
        let findings = self.view_findings();
        let mut rows: Vec<PortRow> = devices
            .iter()
            .map(|d| self.build_row(d, assignments.get(&d.id), &findings).0)
            .collect();
        rows.sort_by(|a, b| {
            b.status
                .is_connected()
                .cmp(&a.status.is_connected())
                .then_with(|| {
                    let ka = a
                        .port
                        .as_deref()
                        .map(cominspect_core::model::natural_sort_key);
                    let kb = b
                        .port
                        .as_deref()
                        .map(cominspect_core::model::natural_sort_key);
                    ka.cmp(&kb)
                })
                .then(a.device_id.cmp(&b.device_id))
        });

        let visible = rows.iter().filter(|r| !r.ignored);
        let mut summary = Summary::default();
        for row in visible {
            summary.total += 1;
            match row.status {
                PortStatus::Connected => summary.connected += 1,
                PortStatus::Problem => {
                    summary.connected += 1;
                    summary.problems += 1;
                }
                PortStatus::Awaiting => {
                    summary.awaiting += 1;
                    summary.disconnected += 1;
                }
                PortStatus::AbsentOs | PortStatus::Absent => summary.disconnected += 1,
            }
            if row.severity == Some(Severity::Warning) {
                summary.warnings += 1;
            }
        }
        summary.ignored = rows.iter().filter(|r| r.ignored).count();

        let scan = self.last_scan.as_ref().map(|s| ScanMeta {
            platform: s.platform.clone(),
            scanned_at: s.scanned_at,
            duration_ms: s.duration_ms,
            warnings: s.warnings.clone(),
            capabilities: s.capabilities.clone(),
            reserved: s.reserved.clone(),
        });
        Ok(InventoryView {
            rows,
            findings,
            summary,
            scan,
            storage: StorageInfo {
                temporary: self.store.temporary_reason().is_some(),
                reason: self.store.temporary_reason().map(str::to_string),
            },
        })
    }

    /// Everything known about one device.
    pub fn detail(&self, device_id: i64) -> Result<DeviceDetail> {
        let d = self.load_device(device_id)?;
        let assignments = self.load_assignments()?;
        let findings = self.view_findings();
        let (row, snapshot) = self.build_row(&d, assignments.get(&d.id), &findings);

        let mut stmt = self.store.conn.prepare(
            "SELECT kind, value, strength, first_seen, last_seen FROM identity_keys
             WHERE device_id = ?1 ORDER BY strength DESC, kind, value",
        )?;
        let keys: Vec<KeyRecord> = stmt
            .query_map(params![device_id], |r| {
                let kind: String = r.get(0)?;
                let parsed = KeyKind::parse(&kind);
                Ok(KeyRecord {
                    portable: parsed.is_some_and(|k| k.portable()),
                    description: parsed.map(|k| k.description()).unwrap_or("").to_string(),
                    kind,
                    value: r.get(1)?,
                    strength: r.get::<_, i64>(2)?.clamp(0, 100) as u8,
                    first_seen: r.get(3)?,
                    last_seen: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let current_port = row.port.clone();
        let port_history: Vec<PortHistoryEntry> = assignments
            .get(&d.id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut e| {
                e.current = current_port
                    .as_deref()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&e.port));
                e
            })
            .collect();

        let mut stmt = self.store.conn.prepare(
            "SELECT id, ts, device_id, kind, port_name, previous_port, detail FROM events
             WHERE device_id = ?1 ORDER BY ts DESC, id DESC LIMIT 50",
        )?;
        let events: Vec<EventRecord> = stmt
            .query_map(params![device_id], |r| {
                Ok(EventRecord {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    device_id: r.get(2)?,
                    kind: r.get(3)?,
                    port: r.get(4)?,
                    previous_port: r.get(5)?,
                    detail: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let hints = snapshot
            .as_ref()
            .map(|s| self.hints.hints_for(s, &self.platform))
            .unwrap_or_default();
        let device_findings: Vec<ViewFinding> = findings
            .into_iter()
            .filter(|f| f.device_ids.contains(&device_id))
            .collect();

        let recognized_by = self.basis.get(&device_id).copied();
        let identity_note = identity_note(&keys, recognized_by, &self.platform);
        let merge_candidates = self.merge_candidates(&d, row.status)?;
        let imported_from = d
            .imported_from
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok());

        Ok(DeviceDetail {
            snapshot_live: self.live_port(device_id).is_some_and(|p| p.is_present()),
            row,
            snapshot,
            recognized_by,
            identity_note,
            keys,
            port_history,
            events,
            hints,
            findings: device_findings,
            merge_candidates,
            imported_from,
        })
    }

    fn merge_candidates(
        &self,
        d: &DeviceRecord,
        status: PortStatus,
    ) -> Result<Vec<MergeCandidate>> {
        let devices = self.load_devices()?;
        let this_connected = status.is_connected();
        let this_serial = d.serial_number.as_deref().and_then(usable_serial);
        let mut out: Vec<MergeCandidate> = Vec::new();
        for other in devices.iter().filter(|o| o.id != d.id) {
            let other_live = self.live_port(other.id);
            let other_connected = other_live.is_some_and(|p| p.is_present());
            if this_connected && other_connected {
                continue;
            }
            if d.transport != Transport::Unknown
                && other.transport != Transport::Unknown
                && d.transport != other.transport
            {
                continue;
            }
            let same_ids = match (d.vid, d.pid, other.vid, other.pid) {
                (Some(a), Some(b), Some(c), Some(e)) => {
                    if (a, b) != (c, e) {
                        continue;
                    }
                    true
                }
                _ => false,
            };
            let other_serial = other.serial_number.as_deref().and_then(usable_serial);
            if let (Some(a), Some(b)) = (&this_serial, &other_serial)
                && a != b
            {
                continue;
            }
            if d.interface_number.is_some()
                && other.interface_number.is_some()
                && d.interface_number != other.interface_number
            {
                continue;
            }
            let reason = if same_ids {
                format!(
                    "Same USB device type ({:04X}:{:04X})",
                    d.vid.unwrap_or_default(),
                    d.pid.unwrap_or_default()
                )
            } else {
                format!("Same connection type ({})", other.transport.label())
            };
            let other_status = match other_live {
                Some(p) if p.is_present() => PortStatus::Connected,
                Some(_) => PortStatus::AbsentOs,
                None if other.awaiting => PortStatus::Awaiting,
                None => PortStatus::Absent,
            };
            let fallback = other
                .description
                .clone()
                .or_else(|| other.product.clone())
                .unwrap_or_else(|| "Serial device".to_string());
            out.push(MergeCandidate {
                device_id: other.id,
                label: record_label(&other.identity, &fallback),
                port: other.last_port.clone(),
                status: other_status,
                last_seen: other.last_seen,
                reason,
            });
        }
        out.sort_by(|a, b| {
            b.last_seen
                .cmp(&a.last_seen)
                .then(a.device_id.cmp(&b.device_id))
        });
        out.truncate(25);
        Ok(out)
    }

    // --- mutations ----------------------------------------------------------

    pub fn update_identity(
        &mut self,
        device_id: i64,
        patch: &IdentityPatch,
    ) -> Result<UserIdentity> {
        let d = self.load_device(device_id)?;
        let mut identity = d.identity.clone();
        patch.apply(&mut identity);
        self.store.conn.execute(
            "UPDATE devices SET nickname = ?2, equipment = ?3, category = ?4, purpose = ?5,
                    cat_status = ?6, notes = ?7, updated_at = ?8
             WHERE id = ?1",
            params![
                device_id,
                identity.nickname,
                identity.equipment,
                identity.category.map(|c| c.as_str()),
                identity.purpose.map(|p| p.as_str()),
                identity.cat_status.as_str(),
                identity.notes,
                now_ms()
            ],
        )?;
        Ok(identity)
    }

    pub fn set_ignored(&mut self, device_id: i64, ignored: bool) -> Result<()> {
        let changed = self.store.conn.execute(
            "UPDATE devices SET ignored = ?2, updated_at = ?3 WHERE id = ?1",
            params![device_id, ignored as i64, now_ms()],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(device_id));
        }
        Ok(())
    }

    /// Links `source` into `target` ("Same device as…"): identities, keys,
    /// port history and events are combined and `source` is removed.
    pub fn merge(&mut self, target: i64, source: i64) -> Result<()> {
        if target == source {
            return Err(StoreError::Invalid(
                "a device cannot be merged with itself".into(),
            ));
        }
        let t = self.load_device(target)?;
        let s = self.load_device(source)?;
        let t_live = self.live_port(target).is_some_and(|p| p.is_present());
        let s_live = self.live_port(source).is_some_and(|p| p.is_present());
        if t_live && s_live {
            return Err(StoreError::Invalid(
                "Both devices are connected right now, so they cannot be the same device.".into(),
            ));
        }
        let mut identity = t.identity.clone();
        identity.fill_from(&s.identity);
        let now = now_ms();
        // The more recently observed record describes the current state.
        let (fresh, _) = if s.last_observed > t.last_observed {
            (&s, &t)
        } else {
            (&t, &s)
        };

        let tx = self.store.conn.transaction()?;
        tx.execute(
            "INSERT INTO identity_keys(device_id, kind, value, strength, first_seen, last_seen)
             SELECT ?1, kind, value, strength, first_seen, last_seen FROM identity_keys WHERE device_id = ?2
             ON CONFLICT(device_id, kind, value) DO UPDATE SET
                first_seen = MIN(identity_keys.first_seen, excluded.first_seen),
                last_seen = MAX(identity_keys.last_seen, excluded.last_seen)",
            params![target, source],
        )?;
        tx.execute(
            "INSERT INTO port_assignments(device_id, port_name, first_observed, last_observed, last_connected)
             SELECT ?1, port_name, first_observed, last_observed, last_connected FROM port_assignments WHERE device_id = ?2
             ON CONFLICT(device_id, port_name) DO UPDATE SET
                first_observed = MIN(port_assignments.first_observed, excluded.first_observed),
                last_observed = MAX(port_assignments.last_observed, excluded.last_observed),
                last_connected = MAX(COALESCE(port_assignments.last_connected, 0), COALESCE(excluded.last_connected, 0))",
            params![target, source],
        )?;
        tx.execute(
            "UPDATE events SET device_id = ?1 WHERE device_id = ?2",
            params![target, source],
        )?;
        tx.execute(
            "UPDATE devices SET
                nickname = ?2, equipment = ?3, category = ?4, purpose = ?5, cat_status = ?6, notes = ?7,
                ignored = ?8,
                vid = COALESCE(vid, ?9), pid = COALESCE(pid, ?10), serial_number = COALESCE(serial_number, ?11),
                interface_number = COALESCE(interface_number, ?12), manufacturer = COALESCE(manufacturer, ?13),
                product = COALESCE(product, ?14), description = COALESCE(description, ?15),
                bt_address = COALESCE(bt_address, ?16), virtual_provider = COALESCE(virtual_provider, ?17),
                last_snapshot = COALESCE(last_snapshot, ?18),
                snapshot_presence = COALESCE(snapshot_presence, ?19),
                snapshot_hash = COALESCE(snapshot_hash, ?20),
                last_port = ?21, connected = ?22,
                first_seen = MIN(first_seen, ?23),
                last_seen = CASE WHEN last_seen IS NULL THEN ?24 WHEN ?24 IS NULL THEN last_seen ELSE MAX(last_seen, ?24) END,
                last_observed = MAX(last_observed, ?25),
                awaiting = CASE WHEN awaiting = 1 AND ?26 = 1 THEN 1 ELSE 0 END,
                imported_from = COALESCE(imported_from, ?27),
                updated_at = ?28
             WHERE id = ?1",
            params![
                target,
                identity.nickname,
                identity.equipment,
                identity.category.map(|c| c.as_str()),
                identity.purpose.map(|p| p.as_str()),
                identity.cat_status.as_str(),
                identity.notes,
                (t.ignored && s.ignored) as i64,
                s.vid,
                s.pid,
                s.serial_number,
                s.interface_number,
                s.manufacturer,
                s.product,
                s.description,
                s.bt_address,
                s.virtual_provider,
                s.snapshot_json,
                s.snapshot_presence,
                s.snapshot_hash,
                fresh.last_port,
                (t.connected || s.connected) as i64,
                s.first_seen,
                s.last_seen,
                s.last_observed,
                s.awaiting as i64,
                s.imported_from,
                now
            ],
        )?;
        tx.execute("DELETE FROM devices WHERE id = ?1", params![source])?;
        let source_label = record_label(
            &s.identity,
            s.description
                .as_deref()
                .or(s.last_port.as_deref())
                .unwrap_or("device"),
        );
        tx.execute(
            "INSERT INTO events(ts, device_id, kind, port_name, previous_port, detail)
             VALUES (?1, ?2, 'merged', ?3, NULL, ?4)",
            params![
                now,
                target,
                s.last_port,
                format!("Linked with {source_label}")
            ],
        )?;
        tx.commit()?;

        if let Some(index) = self.scan_index.remove(&source) {
            self.scan_index.entry(target).or_insert(index);
        }
        if let Some(b) = self.basis.remove(&source) {
            self.basis.entry(target).or_insert(b);
        }
        Ok(())
    }

    /// Deletes a device and its history. If it is connected it will be
    /// recorded again as a new device on the next scan.
    pub fn forget(&mut self, device_id: i64) -> Result<()> {
        let changed = self
            .store
            .conn
            .execute("DELETE FROM devices WHERE id = ?1", params![device_id])?;
        if changed == 0 {
            return Err(StoreError::NotFound(device_id));
        }
        self.scan_index.remove(&device_id);
        self.basis.remove(&device_id);
        Ok(())
    }

    /// Recent events across all devices.
    pub fn recent_events(&self, limit: usize) -> Result<Vec<EventRecord>> {
        let mut stmt = self.store.conn.prepare(
            "SELECT id, ts, device_id, kind, port_name, previous_port, detail FROM events
             ORDER BY ts DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(EventRecord {
                id: r.get(0)?,
                ts: r.get(1)?,
                device_id: r.get(2)?,
                kind: r.get(3)?,
                port: r.get(4)?,
                previous_port: r.get(5)?,
                detail: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Counts for the About dialog.
    pub fn stats(&self) -> Result<(i64, i64, i64)> {
        let conn = &self.store.conn;
        let devices = conn.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))?;
        let keys = conn.query_row("SELECT COUNT(*) FROM identity_keys", [], |r| r.get(0))?;
        let events = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
        Ok((devices, keys, events))
    }

    /// Device IDs currently present in the last scan (for tests and callers).
    pub fn present_device_ids(&self) -> HashSet<i64> {
        self.scan_index
            .keys()
            .copied()
            .filter(|id| self.live_port(*id).is_some_and(|p| p.is_present()))
            .collect()
    }
}

fn identity_note(keys: &[KeyRecord], basis: Option<MatchBasis>, platform: &str) -> String {
    let has = |kind: KeyKind| keys.iter().any(|k| k.kind == kind.as_str());
    let instance = if platform == "windows" {
        "Windows device instance ID"
    } else {
        "operating-system device path"
    };
    let mut note = if has(KeyKind::UsbSerial) {
        "Recognized by its USB serial number, so it is identified in any USB socket, whatever COM \
         number it receives, and also on other computers after an import."
            .to_string()
    } else if has(KeyKind::Bluetooth) {
        "Recognized by the Bluetooth address of the paired device.".to_string()
    } else if has(KeyKind::UsbPath) {
        "This device has no unique serial number, so ComInspect recognizes it by the USB socket it \
         is plugged into. If you move it to another socket it will appear as a new entry; use \
         \"Same device as…\" to link that entry to this one."
            .to_string()
    } else if has(KeyKind::OsDevice) {
        format!("Recognized by its {instance}.")
    } else if has(KeyKind::Virtual) {
        "Virtual port, recognized by its provider and port name.".to_string()
    } else {
        "Recognized only by its port name. If the name changes it will appear as a new entry."
            .to_string()
    };
    if let Some(b) = basis {
        note.push_str(&format!(
            " Matched in the latest scan by {}.",
            b.description()
        ));
    }
    note
}

#[allow(clippy::too_many_arguments)]
fn insert_device(
    tx: &Transaction<'_>,
    port: &DiscoveredPort,
    present: bool,
    json: &str,
    hash: &str,
    s: &HwSummary,
    now: i64,
) -> Result<i64> {
    let uuid = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO devices(uuid, transport, vid, pid, serial_number, interface_number, manufacturer,
             product, description, bt_address, virtual_provider, last_snapshot, snapshot_presence,
             snapshot_hash, last_port, connected, first_seen, last_seen, last_observed, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?17, ?17, ?17)",
        params![
            uuid,
            s.transport.as_str(),
            s.vid,
            s.pid,
            s.serial,
            s.interface,
            s.manufacturer,
            s.product,
            s.description,
            s.bt_address,
            s.virtual_provider,
            json,
            if present { "present" } else { "absent" },
            hash,
            port.port_name,
            present as i64,
            now,
            present.then_some(now),
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
fn update_observed(
    tx: &Transaction<'_>,
    rec: &DeviceRecord,
    port: &DiscoveredPort,
    present: bool,
    json: &str,
    hash: &str,
    s: &HwSummary,
    now: i64,
) -> Result<()> {
    let same_port = rec.last_port.as_deref() == Some(port.port_name.as_str());
    if present {
        let unchanged = rec.connected
            && same_port
            && !rec.awaiting
            && rec.snapshot_hash.as_deref() == Some(hash)
            && rec.last_seen.is_some_and(|t| now - t < TOUCH_INTERVAL_MS);
        if unchanged {
            return Ok(());
        }
        tx.execute(
            "UPDATE devices SET connected = 1, last_port = ?2, last_observed = ?3, last_seen = ?3,
                awaiting = 0, transport = ?4, vid = COALESCE(?5, vid), pid = COALESCE(?6, pid),
                serial_number = COALESCE(?7, serial_number),
                interface_number = COALESCE(?8, interface_number),
                manufacturer = COALESCE(?9, manufacturer), product = COALESCE(?10, product),
                description = COALESCE(?11, description), bt_address = COALESCE(?12, bt_address),
                virtual_provider = COALESCE(?13, virtual_provider),
                last_snapshot = ?14, snapshot_presence = 'present', snapshot_hash = ?15,
                updated_at = ?3
             WHERE id = ?1",
            params![
                rec.id,
                port.port_name,
                now,
                s.transport.as_str(),
                s.vid,
                s.pid,
                s.serial,
                s.interface,
                s.manufacturer,
                s.product,
                s.description,
                s.bt_address,
                s.virtual_provider,
                json,
                hash
            ],
        )?;
    } else {
        let snapshot_is_rich = rec.snapshot_presence.as_deref() == Some("present");
        let unchanged = !rec.connected
            && same_port
            && !rec.awaiting
            && (snapshot_is_rich || rec.snapshot_hash.as_deref() == Some(hash))
            && now - rec.last_observed < ABSENT_TOUCH_INTERVAL_MS;
        if unchanged {
            return Ok(());
        }
        tx.execute(
            "UPDATE devices SET connected = 0, last_port = ?2, last_observed = ?3, awaiting = 0,
                transport = CASE WHEN transport = 'unknown' THEN ?4 ELSE transport END,
                vid = COALESCE(vid, ?5), pid = COALESCE(pid, ?6),
                serial_number = COALESCE(serial_number, ?7),
                interface_number = COALESCE(interface_number, ?8),
                manufacturer = COALESCE(manufacturer, ?9), product = COALESCE(product, ?10),
                description = COALESCE(description, ?11), bt_address = COALESCE(bt_address, ?12),
                virtual_provider = COALESCE(virtual_provider, ?13),
                last_snapshot = CASE WHEN snapshot_presence = 'present' THEN last_snapshot ELSE ?14 END,
                snapshot_hash = CASE WHEN snapshot_presence = 'present' THEN snapshot_hash ELSE ?15 END,
                snapshot_presence = CASE WHEN snapshot_presence = 'present' THEN 'present' ELSE 'absent' END,
                updated_at = ?3
             WHERE id = ?1",
            params![
                rec.id,
                port.port_name,
                now,
                s.transport.as_str(),
                s.vid,
                s.pid,
                s.serial,
                s.interface,
                s.manufacturer,
                s.product,
                s.description,
                s.bt_address,
                s.virtual_provider,
                json,
                hash
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn upsert_keys(
    tx: &Transaction<'_>,
    device_id: i64,
    keys: &[IdentityKey],
    now: i64,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO identity_keys(device_id, kind, value, strength, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(device_id, kind, value) DO UPDATE SET
            last_seen = excluded.last_seen, strength = excluded.strength
         WHERE excluded.last_seen - identity_keys.last_seen >= ?6
            OR identity_keys.strength != excluded.strength",
    )?;
    for key in keys {
        stmt.execute(params![
            device_id,
            key.kind.as_str(),
            key.value,
            key.strength,
            now,
            TOUCH_INTERVAL_MS
        ])?;
    }
    Ok(())
}

fn upsert_assignment(
    tx: &Transaction<'_>,
    device_id: i64,
    port: &str,
    present: bool,
    now: i64,
) -> Result<()> {
    tx.prepare_cached(
        "INSERT INTO port_assignments(device_id, port_name, first_observed, last_observed, last_connected)
         VALUES (?1, ?2, ?3, ?3, ?4)
         ON CONFLICT(device_id, port_name) DO UPDATE SET
            last_observed = excluded.last_observed,
            last_connected = COALESCE(excluded.last_connected, port_assignments.last_connected)
         WHERE excluded.last_observed - port_assignments.last_observed >= ?5
            OR (excluded.last_connected IS NOT NULL
                AND (port_assignments.last_connected IS NULL
                     OR excluded.last_connected - port_assignments.last_connected >= ?5))",
    )?
    .execute(params![
        device_id,
        port,
        now,
        present.then_some(now),
        TOUCH_INTERVAL_MS
    ])?;
    Ok(())
}
