//! Known-device hint database.
//!
//! Maps USB IDs, interface numbers, serial prefixes and other evidence to
//! human-friendly hints such as "CP2105 Enhanced COM port". Hints are only
//! *suggestions*: they are displayed to the user and can be applied with one
//! click, but they never change a device's purpose or CAT status on their own.
//!
//! The bundled database is compiled into the binary. The format is versioned
//! so that a community-maintained, signed feed can be layered on top later
//! (see `docs/ARCHITECTURE.md` §11).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::identity::normalize_serial;
use crate::model::{DiscoveredPort, Transport};
use crate::user::{Category, Purpose};

const BUNDLED: &str = include_str!("../data/known-devices.json");

/// Highest `schemaVersion` this build understands.
pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum HintDbError {
    #[error("invalid device hint database: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("device hint database schema {found} is newer than supported ({SUPPORTED_SCHEMA})")]
    UnsupportedSchema { found: u32 },
    #[error("invalid hex value {0:?} in device hint database")]
    BadHex(String),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Matcher {
    vid: Option<String>,
    pid: Option<String>,
    interface: Option<u8>,
    serial_prefix: Option<String>,
    product_contains: Option<String>,
    description_contains: Option<String>,
    name_contains: Option<String>,
    transport: Option<Transport>,
    driver: Option<String>,
    platform: Option<String>,
    virtual_provider: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    id: String,
    #[serde(rename = "match")]
    matcher: Matcher,
    chip: Option<String>,
    title: String,
    detail: Option<String>,
    short_label: Option<String>,
    equipment: Option<String>,
    purpose: Option<Purpose>,
    category: Option<Category>,
    #[serde(default)]
    caution: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbFile {
    schema_version: u32,
    name: String,
    entries: Vec<Entry>,
}

/// A compiled entry: hex IDs parsed once.
#[derive(Clone, Debug)]
struct Compiled {
    entry: Entry,
    vid: Option<u16>,
    pid: Option<u16>,
    specificity: usize,
}

/// A hint shown in the inspector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub id: String,
    pub title: String,
    pub detail: Option<String>,
    /// Compact label for the device column ("Enhanced COM port").
    pub short_label: Option<String>,
    pub chip: Option<String>,
    pub suggested_purpose: Option<Purpose>,
    pub suggested_category: Option<Category>,
    pub suggested_equipment: Option<String>,
    /// The hint warns about a potential problem.
    pub caution: bool,
    pub source: String,
}

/// Parsed device hint database.
#[derive(Clone, Debug)]
pub struct KnownDeviceDb {
    name: String,
    entries: Vec<Compiled>,
}

fn parse_hex(value: &str) -> Result<u16, HintDbError> {
    u16::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|_| HintDbError::BadHex(value.to_string()))
}

fn contains_ci(haystack: Option<&str>, needle: &str) -> bool {
    haystack.is_some_and(|h| h.to_lowercase().contains(&needle.to_lowercase()))
}

impl KnownDeviceDb {
    pub fn parse(json: &str) -> Result<KnownDeviceDb, HintDbError> {
        let file: DbFile = serde_json::from_str(json)?;
        if file.schema_version > SUPPORTED_SCHEMA {
            return Err(HintDbError::UnsupportedSchema {
                found: file.schema_version,
            });
        }
        let mut entries = Vec::with_capacity(file.entries.len());
        for entry in file.entries {
            let m = &entry.matcher;
            let specificity = [
                m.vid.is_some(),
                m.pid.is_some(),
                m.interface.is_some(),
                m.serial_prefix.is_some(),
                m.product_contains.is_some(),
                m.description_contains.is_some(),
                m.name_contains.is_some(),
                m.transport.is_some(),
                m.driver.is_some(),
                m.platform.is_some(),
                m.virtual_provider.is_some(),
            ]
            .into_iter()
            .filter(|set| *set)
            .count();
            entries.push(Compiled {
                vid: m.vid.as_deref().map(parse_hex).transpose()?,
                pid: m.pid.as_deref().map(parse_hex).transpose()?,
                specificity,
                entry,
            });
        }
        Ok(KnownDeviceDb {
            name: file.name,
            entries,
        })
    }

    /// The database compiled into this build.
    pub fn bundled() -> &'static KnownDeviceDb {
        static DB: OnceLock<KnownDeviceDb> = OnceLock::new();
        DB.get_or_init(|| KnownDeviceDb::parse(BUNDLED).expect("bundled hint database is valid"))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn matches(c: &Compiled, port: &DiscoveredPort, platform: &str) -> bool {
        let m = &c.entry.matcher;
        let usb = port.usb.as_ref();
        if let Some(vid) = c.vid
            && usb.map(|u| u.vid) != Some(vid)
        {
            return false;
        }
        if let Some(pid) = c.pid
            && usb.map(|u| u.pid) != Some(pid)
        {
            return false;
        }
        if let Some(iface) = m.interface
            && usb.and_then(|u| u.interface_number).unwrap_or(0) != iface
        {
            return false;
        }
        if let Some(prefix) = &m.serial_prefix {
            let serial = usb
                .and_then(|u| u.serial_number.as_deref())
                .map(normalize_serial);
            if !serial.is_some_and(|s| s.starts_with(&prefix.to_uppercase())) {
                return false;
            }
        }
        if let Some(needle) = &m.product_contains
            && !contains_ci(usb.and_then(|u| u.product.as_deref()), needle)
        {
            return false;
        }
        if let Some(needle) = &m.description_contains
            && !contains_ci(port.description.as_deref(), needle)
            && !contains_ci(port.friendly_name.as_deref(), needle)
        {
            return false;
        }
        if let Some(needle) = &m.name_contains
            && !contains_ci(Some(&port.port_name), needle)
        {
            return false;
        }
        if let Some(transport) = m.transport
            && port.transport != transport
        {
            return false;
        }
        if let Some(driver) = &m.driver
            && !port
                .system
                .driver
                .as_deref()
                .is_some_and(|d| d.eq_ignore_ascii_case(driver))
        {
            return false;
        }
        if let Some(p) = &m.platform
            && !p.eq_ignore_ascii_case(platform)
        {
            return false;
        }
        if let Some(provider) = &m.virtual_provider
            && !contains_ci(
                port.virtual_port.as_ref().map(|v| v.provider.as_str()),
                provider,
            )
        {
            return false;
        }
        true
    }

    /// All hints matching the port, most specific first.
    pub fn hints_for(&self, port: &DiscoveredPort, platform: &str) -> Vec<Hint> {
        let serial_model = port
            .usb
            .as_ref()
            .and_then(|u| u.serial_number.as_deref())
            .map(normalize_serial)
            .and_then(|s| s.split('_').next().map(str::to_string))
            .unwrap_or_default();
        let product = port
            .usb
            .as_ref()
            .and_then(|u| u.product.clone())
            .unwrap_or_default();
        let render = |template: &str| {
            template
                .replace("{serialModel}", &serial_model)
                .replace("{product}", &product)
        };

        let mut matched: Vec<&Compiled> = self
            .entries
            .iter()
            .filter(|c| Self::matches(c, port, platform))
            .collect();
        matched.sort_by_key(|c| std::cmp::Reverse(c.specificity));

        matched
            .into_iter()
            .map(|c| {
                let e = &c.entry;
                Hint {
                    id: e.id.clone(),
                    title: render(&e.title),
                    detail: e.detail.as_deref().map(render),
                    short_label: e.short_label.as_deref().map(render),
                    chip: e.chip.clone(),
                    suggested_purpose: e.purpose,
                    suggested_category: e.category,
                    suggested_equipment: e.equipment.as_deref().map(render),
                    caution: e.caution,
                    source: self.name.clone(),
                }
            })
            .collect()
    }

    /// Chip or product family name for a USB ID, if known.
    pub fn chip_for(&self, vid: u16, pid: u16) -> Option<&str> {
        self.entries
            .iter()
            .filter(|c| c.vid == Some(vid) && c.pid == Some(pid))
            .find_map(|c| c.entry.chip.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SystemInfo, UsbInfo, VirtualInfo};

    fn usb(vid: u16, pid: u16, serial: Option<&str>, iface: Option<u8>) -> DiscoveredPort {
        DiscoveredPort {
            port_name: "COM7".into(),
            transport: Transport::Usb,
            usb: Some(UsbInfo {
                vid,
                pid,
                serial_number: serial.map(Into::into),
                interface_number: iface,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn bundled_database_parses() {
        let db = KnownDeviceDb::bundled();
        assert!(db.len() > 20);
    }

    #[test]
    fn cp2105_interfaces_are_labelled() {
        let db = KnownDeviceDb::bundled();
        let enhanced = db.hints_for(&usb(0x10C4, 0xEA70, Some("01A2B3C4"), Some(0)), "windows");
        assert_eq!(
            enhanced[0].short_label.as_deref(),
            Some("Enhanced COM port")
        );
        assert_eq!(enhanced[0].suggested_purpose, Some(Purpose::Cat));
        assert_eq!(enhanced[1].id, "silabs-cp2105");

        let standard = db.hints_for(&usb(0x10C4, 0xEA70, Some("01A2B3C4"), Some(1)), "linux");
        assert_eq!(
            standard[0].short_label.as_deref(),
            Some("Standard COM port")
        );
        assert_eq!(standard[0].suggested_purpose, Some(Purpose::Ptt));
        assert_eq!(
            db.chip_for(0x10C4, 0xEA70),
            Some("Silicon Labs CP2105 dual UART")
        );
    }

    #[test]
    fn icom_radio_is_identified_from_serial_string() {
        let db = KnownDeviceDb::bundled();
        let hints = db.hints_for(
            &usb(0x10C4, 0xEA60, Some("IC-7300 03001234"), None),
            "macos",
        );
        assert_eq!(hints[0].title, "Icom IC-7300");
        assert_eq!(
            hints[0].suggested_equipment.as_deref(),
            Some("Icom IC-7300")
        );
        assert_eq!(
            hints[0].suggested_purpose, None,
            "never claims CAT on its own"
        );
        assert!(hints.iter().any(|h| h.id == "silabs-cp210x"));

        let plain = db.hints_for(&usb(0x10C4, 0xEA60, Some("0001"), None), "macos");
        assert!(plain.iter().all(|h| h.id != "icom-cp210x-serial"));
    }

    #[test]
    fn non_usb_matchers() {
        let db = KnownDeviceDb::bundled();
        let amt = DiscoveredPort {
            port_name: "COM3".into(),
            transport: Transport::Pci,
            friendly_name: Some("Intel(R) Active Management Technology - SOL (COM3)".into()),
            ..Default::default()
        };
        assert_eq!(db.hints_for(&amt, "windows")[0].id, "intel-amt-sol");

        let vport = DiscoveredPort {
            port_name: "COM12".into(),
            transport: Transport::Virtual,
            virtual_port: Some(VirtualInfo {
                provider: "com0com".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(db.hints_for(&vport, "windows")[0].id, "com0com");

        let acm = DiscoveredPort {
            port_name: "/dev/ttyACM0".into(),
            transport: Transport::Usb,
            system: SystemInfo {
                driver: Some("cdc_acm".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            db.hints_for(&acm, "linux")
                .iter()
                .any(|h| h.id == "linux-cdc-acm-modemmanager" && h.caution)
        );
        assert!(db.hints_for(&acm, "windows").is_empty());
    }

    #[test]
    fn rejects_newer_schema() {
        let err = KnownDeviceDb::parse(r#"{"schemaVersion": 99, "name": "x", "entries": []}"#)
            .unwrap_err();
        assert!(matches!(err, HintDbError::UnsupportedSchema { found: 99 }));
    }
}
