//! Export and import of the portable inventory file.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};

use cominspect_core::export::{
    EXPORT_FORMAT, EXPORT_FORMAT_VERSION, ExportDevice, ExportDocument, ExportHardware,
    ExportIdentity, ExportKey, ExportMachine, ExportPortAssignment, ExportSource, ImportMode,
};
use cominspect_core::identity::{IdentityKey, KeyKind, normalize_bt_address};
use cominspect_core::model::Transport;
use cominspect_core::time::{now_ms, to_rfc3339};
use cominspect_core::user::UserIdentity;

use crate::error::Result;
use crate::inventory::{Inventory, upsert_keys};
use crate::view::{ImportReport, ImportedFrom};

fn hex(value: Option<u16>) -> Option<String> {
    value.map(|v| format!("{v:04X}"))
}

fn parse_hex(value: Option<&str>) -> Option<u16> {
    value.and_then(|v| u16::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())
}

impl Inventory {
    /// Builds the export document for all devices.
    pub fn export(&self, app_version: &str, hostname: Option<&str>) -> Result<ExportDocument> {
        let devices = self.load_devices()?;
        let conn = &self.store.conn;

        let mut keys: HashMap<i64, Vec<ExportKey>> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT device_id, kind, value, strength FROM identity_keys ORDER BY device_id, strength DESC, kind, value",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (id, kind, value, strength) = row?;
            let portable = KeyKind::parse(&kind).is_some_and(|k| k.portable());
            keys.entry(id).or_default().push(ExportKey {
                kind,
                value,
                strength: strength.clamp(0, 100) as u8,
                portable,
            });
        }

        let mut history: HashMap<i64, Vec<ExportPortAssignment>> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT device_id, port_name, first_observed, last_observed FROM port_assignments
             ORDER BY device_id, first_observed",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (id, port, first, last) = row?;
            history.entry(id).or_default().push(ExportPortAssignment {
                port,
                first_observed: Some(to_rfc3339(first)),
                last_observed: Some(to_rfc3339(last)),
            });
        }

        let devices = devices
            .into_iter()
            .map(|d| ExportDevice {
                uuid: d.uuid.clone(),
                identity: ExportIdentity {
                    nickname: d.identity.nickname.clone(),
                    equipment: d.identity.equipment.clone(),
                    category: d.identity.category,
                    purpose: d.identity.purpose,
                    cat_status: d.identity.cat_status,
                    notes: d.identity.notes.clone(),
                    ignored: d.ignored,
                },
                hardware: ExportHardware {
                    transport: d.transport.as_str().to_string(),
                    vid: hex(d.vid),
                    pid: hex(d.pid),
                    serial: d.serial_number.clone(),
                    interface: d.interface_number,
                    manufacturer: d.manufacturer.clone(),
                    product: d.product.clone(),
                    description: d.description.clone(),
                    bt_address: d.bt_address.clone(),
                    virtual_provider: d.virtual_provider.clone(),
                    keys: keys.remove(&d.id).unwrap_or_default(),
                },
                machine: ExportMachine {
                    last_port: d.last_port.clone(),
                    port_history: history.remove(&d.id).unwrap_or_default(),
                    first_seen: Some(to_rfc3339(d.first_seen)),
                    last_seen: d.last_seen.map(to_rfc3339),
                },
            })
            .collect();

        Ok(ExportDocument {
            format: EXPORT_FORMAT.to_string(),
            format_version: EXPORT_FORMAT_VERSION,
            exported_at: to_rfc3339(now_ms()),
            app_version: app_version.to_string(),
            source: ExportSource {
                os: self.platform().to_string(),
                hostname: hostname.map(str::to_string),
            },
            devices,
        })
    }

    /// Imports an export document. Devices are matched by UUID, then by
    /// portable identity keys (and, when the file comes from this same
    /// computer, by all keys). Unmatched devices are stored as "awaiting"
    /// identities that will be recognized when the hardware appears.
    pub fn import(
        &mut self,
        doc: &ExportDocument,
        mode: ImportMode,
        hostname: Option<&str>,
    ) -> Result<ImportReport> {
        let same_machine = doc.source.os == self.platform()
            && match (doc.source.hostname.as_deref(), hostname) {
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            };
        let mut report = ImportReport {
            same_machine,
            source_hostname: doc.source.hostname.clone(),
            source_os: doc.source.os.clone(),
            ..Default::default()
        };
        let now = now_ms();
        let tx = self.store.conn.transaction()?;

        for device in &doc.devices {
            let usable_keys: Vec<IdentityKey> = device
                .hardware
                .keys
                .iter()
                .filter(|k| k.portable || same_machine)
                .filter_map(|k| {
                    KeyKind::parse(&k.kind)
                        .map(|kind| IdentityKey::new(kind, k.value.clone(), k.strength))
                })
                .collect();

            let mut target: Option<i64> = tx
                .query_row(
                    "SELECT id FROM devices WHERE uuid = ?1",
                    params![device.uuid],
                    |r| r.get(0),
                )
                .optional()?;
            if target.is_none() {
                for key in usable_keys.iter().filter(|k| k.strength >= 50) {
                    let found: Option<i64> = tx
                        .query_row(
                            "SELECT device_id FROM identity_keys WHERE kind = ?1 AND value = ?2
                             ORDER BY last_seen DESC LIMIT 1",
                            params![key.kind.as_str(), key.value],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if found.is_some() {
                        target = found;
                        break;
                    }
                }
            }

            let imported = UserIdentity {
                nickname: device.identity.nickname.clone(),
                equipment: device.identity.equipment.clone(),
                category: device.identity.category,
                purpose: device.identity.purpose,
                cat_status: device.identity.cat_status,
                notes: device.identity.notes.clone(),
            };

            let device_id = match target {
                Some(id) => {
                    let existing: UserIdentity = tx.query_row(
                        "SELECT nickname, equipment, category, purpose, cat_status, notes FROM devices WHERE id = ?1",
                        params![id],
                        |r| {
                            Ok(UserIdentity {
                                nickname: r.get(0)?,
                                equipment: r.get(1)?,
                                category: r
                                    .get::<_, Option<String>>(2)?
                                    .as_deref()
                                    .and_then(cominspect_core::user::Category::parse),
                                purpose: r
                                    .get::<_, Option<String>>(3)?
                                    .as_deref()
                                    .and_then(cominspect_core::user::Purpose::parse),
                                cat_status: cominspect_core::user::CatStatus::parse(
                                    &r.get::<_, String>(4)?,
                                )
                                .unwrap_or_default(),
                                notes: r.get(5)?,
                            })
                        },
                    )?;
                    let merged = match mode {
                        ImportMode::Merge => {
                            let mut m = existing.clone();
                            m.fill_from(&imported);
                            m
                        }
                        ImportMode::Overwrite => {
                            let mut m = imported.clone();
                            m.fill_from(&existing);
                            if imported.notes.is_some() {
                                m.notes.clone_from(&imported.notes);
                            }
                            m
                        }
                    };
                    if merged == existing {
                        report.unchanged += 1;
                    } else {
                        report.matched += 1;
                        tx.execute(
                            "UPDATE devices SET nickname = ?2, equipment = ?3, category = ?4, purpose = ?5,
                                cat_status = ?6, notes = ?7, updated_at = ?8 WHERE id = ?1",
                            params![
                                id,
                                merged.nickname,
                                merged.equipment,
                                merged.category.map(|c| c.as_str()),
                                merged.purpose.map(|p| p.as_str()),
                                merged.cat_status.as_str(),
                                merged.notes,
                                now
                            ],
                        )?;
                    }
                    id
                }
                None => {
                    let hw = &device.hardware;
                    let imported_from = ImportedFrom {
                        hostname: doc.source.hostname.clone(),
                        os: doc.source.os.clone(),
                        last_port: device.machine.last_port.clone(),
                        port_history: device
                            .machine
                            .port_history
                            .iter()
                            .map(|p| p.port.clone())
                            .collect(),
                        imported_at: now,
                    };
                    let uuid_taken: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM devices WHERE uuid = ?1)",
                        params![device.uuid],
                        |r| r.get(0),
                    )?;
                    let uuid = if uuid_taken || device.uuid.trim().is_empty() {
                        uuid::Uuid::new_v4().to_string()
                    } else {
                        device.uuid.clone()
                    };
                    let transport = Transport::parse(&hw.transport).unwrap_or_default();
                    tx.execute(
                        "INSERT INTO devices(uuid, nickname, equipment, category, purpose, cat_status, notes, ignored,
                            transport, vid, pid, serial_number, interface_number, manufacturer, product, description,
                            bt_address, virtual_provider, last_port, connected, first_seen, last_seen, last_observed,
                            awaiting, imported_from, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                            ?19, 0, ?20, NULL, ?20, 1, ?21, ?20, ?20)",
                        params![
                            uuid,
                            imported.nickname,
                            imported.equipment,
                            imported.category.map(|c| c.as_str()),
                            imported.purpose.map(|p| p.as_str()),
                            imported.cat_status.as_str(),
                            imported.notes,
                            device.identity.ignored as i64,
                            transport.as_str(),
                            parse_hex(hw.vid.as_deref()),
                            parse_hex(hw.pid.as_deref()),
                            hw.serial,
                            hw.interface,
                            hw.manufacturer,
                            hw.product,
                            hw.description,
                            hw.bt_address.as_deref().and_then(normalize_bt_address),
                            hw.virtual_provider,
                            if same_machine { device.machine.last_port.clone() } else { None },
                            now,
                            serde_json::to_string(&imported_from)?,
                        ],
                    )?;
                    report.created += 1;
                    tx.last_insert_rowid()
                }
            };
            upsert_keys(&tx, device_id, &usable_keys, now)?;
            tx.execute(
                "INSERT INTO events(ts, device_id, kind, port_name, previous_port, detail)
                 VALUES (?1, ?2, 'imported', NULL, NULL, ?3)",
                params![
                    now,
                    device_id,
                    format!(
                        "Imported from {}",
                        doc.source.hostname.as_deref().unwrap_or("another computer")
                    )
                ],
            )?;
        }
        tx.commit()?;
        Ok(report)
    }
}
