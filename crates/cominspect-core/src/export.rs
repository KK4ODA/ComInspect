//! Portable export/import format (`serial-port-inventory.json`).
//!
//! Each device is split into three parts so that moving the file to another
//! computer does the right thing:
//!
//! * `identity` — what the user assigned (nickname, purpose …); portable.
//! * `hardware` — how to recognize the device; only keys flagged `portable`
//!   are meaningful on another computer.
//! * `machine` — the COM assignments of the computer that produced the file;
//!   informational on any other computer.

use serde::{Deserialize, Serialize};

use crate::user::{CatStatus, Category, Purpose};

pub const EXPORT_FORMAT: &str = "cominspect-inventory";
pub const EXPORT_FORMAT_VERSION: u32 = 1;
pub const DEFAULT_EXPORT_FILE_NAME: &str = "serial-port-inventory.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportDocument {
    pub format: String,
    pub format_version: u32,
    /// RFC 3339 timestamp.
    pub exported_at: String,
    pub app_version: String,
    pub source: ExportSource,
    pub devices: Vec<ExportDevice>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSource {
    /// `windows`, `linux` or `macos`.
    pub os: String,
    pub hostname: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportDevice {
    pub uuid: String,
    pub identity: ExportIdentity,
    pub hardware: ExportHardware,
    pub machine: ExportMachine,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportIdentity {
    pub nickname: Option<String>,
    pub equipment: Option<String>,
    pub category: Option<Category>,
    pub purpose: Option<Purpose>,
    #[serde(default)]
    pub cat_status: CatStatus,
    pub notes: Option<String>,
    #[serde(default)]
    pub ignored: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportHardware {
    pub transport: String,
    /// Hex, e.g. `10C4`.
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub serial: Option<String>,
    pub interface: Option<u8>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub description: Option<String>,
    pub bt_address: Option<String>,
    pub virtual_provider: Option<String>,
    pub keys: Vec<ExportKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportKey {
    pub kind: String,
    pub value: String,
    pub strength: u8,
    pub portable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportMachine {
    pub last_port: Option<String>,
    pub port_history: Vec<ExportPortAssignment>,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPortAssignment {
    pub port: String,
    pub first_observed: Option<String>,
    pub last_observed: Option<String>,
}

/// How imported identities are combined with existing ones.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    /// Only fill fields that are empty on this computer.
    #[default]
    Merge,
    /// Replace user-assigned fields with the imported values.
    Overwrite,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExportFormatError {
    #[error("this is not a ComInspect inventory file")]
    WrongFormat,
    #[error(
        "this file was created by a newer version of ComInspect (format {found}); update ComInspect to import it"
    )]
    TooNew { found: u32 },
    #[error("the file could not be read: {0}")]
    Invalid(String),
}

impl ExportDocument {
    /// Parses and validates an export file.
    pub fn from_json(json: &str) -> Result<ExportDocument, ExportFormatError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| ExportFormatError::Invalid(e.to_string()))?;
        if value.get("format").and_then(|f| f.as_str()) != Some(EXPORT_FORMAT) {
            return Err(ExportFormatError::WrongFormat);
        }
        let version = value
            .get("format_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if version > EXPORT_FORMAT_VERSION {
            return Err(ExportFormatError::TooNew { found: version });
        }
        serde_json::from_value(value).map_err(|e| ExportFormatError::Invalid(e.to_string()))
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("export document serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_foreign_and_newer_files() {
        assert_eq!(
            ExportDocument::from_json(r#"{"hello": 1}"#),
            Err(ExportFormatError::WrongFormat)
        );
        assert_eq!(
            ExportDocument::from_json(r#"{"format": "cominspect-inventory", "format_version": 7}"#),
            Err(ExportFormatError::TooNew { found: 7 })
        );
        assert!(matches!(
            ExportDocument::from_json("not json"),
            Err(ExportFormatError::Invalid(_))
        ));
    }

    #[test]
    fn round_trips() {
        let doc = ExportDocument {
            format: EXPORT_FORMAT.into(),
            format_version: EXPORT_FORMAT_VERSION,
            exported_at: "2026-09-26T14:32:17Z".into(),
            app_version: "0.1.0".into(),
            source: ExportSource {
                os: "windows".into(),
                hostname: Some("SHACK-PC".into()),
            },
            devices: vec![ExportDevice {
                uuid: "u1".into(),
                identity: ExportIdentity {
                    nickname: Some("FTDX10 CAT Enhanced".into()),
                    purpose: Some(Purpose::Cat),
                    cat_status: CatStatus::Verified,
                    ..Default::default()
                },
                hardware: ExportHardware {
                    transport: "usb".into(),
                    vid: Some("10C4".into()),
                    keys: vec![ExportKey {
                        kind: "usb-serial".into(),
                        value: "10C4:EA70:01A2B3C4:if0".into(),
                        strength: 100,
                        portable: true,
                    }],
                    ..Default::default()
                },
                machine: ExportMachine {
                    last_port: Some("COM7".into()),
                    ..Default::default()
                },
            }],
        };
        let json = doc.to_json_pretty();
        assert!(json.contains("\"purpose\": \"cat\""));
        assert!(json.contains("\"cat_status\": \"verified\""));
        assert_eq!(ExportDocument::from_json(&json).unwrap(), doc);
    }
}
