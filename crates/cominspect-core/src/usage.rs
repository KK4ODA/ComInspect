//! Which programs have a serial port open.
//!
//! Platform adapters fill a [`UsageSnapshot`] without ever opening a port
//! (opening one can key a transmitter through RTS/DTR). The types here only
//! describe the result and how it changed between two snapshots.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A process that has a port open.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortHolder {
    pub pid: u32,
    /// Executable file name, e.g. `VARAFM.exe`.
    pub process_name: String,
    /// Full path of the executable, when the operating system reveals it.
    pub executable: Option<String>,
    /// Human-readable program name (on Windows the executable's file
    /// description, e.g. "VARA FM").
    pub description: Option<String>,
}

impl PortHolder {
    /// Name to show to people: the description, or the executable name
    /// without a `.exe` suffix.
    pub fn display_name(&self) -> String {
        if let Some(d) = self.description.as_deref().map(str::trim)
            && !d.is_empty()
        {
            return d.to_string();
        }
        let name = self.process_name.as_str();
        match name.len().checked_sub(4) {
            Some(cut) if name[cut..].eq_ignore_ascii_case(".exe") => name[..cut].to_string(),
            _ => name.to_string(),
        }
    }

    /// "VARA FM (VARAFM.exe, PID 4312)".
    pub fn describe(&self) -> String {
        let display = self.display_name();
        if display.eq_ignore_ascii_case(&self.process_name) {
            format!("{display} (PID {})", self.pid)
        } else {
            format!("{display} ({}, PID {})", self.process_name, self.pid)
        }
    }
}

/// Whether a port is open in any program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PortUsage {
    /// No program ComInspect can see has the port open.
    Free,
    InUse {
        holders: Vec<PortHolder>,
    },
    /// The check itself failed.
    Unknown {
        reason: String,
    },
}

impl PortUsage {
    pub fn in_use(holders: Vec<PortHolder>) -> PortUsage {
        if holders.is_empty() {
            PortUsage::Free
        } else {
            let mut holders = holders;
            holders.sort();
            holders.dedup();
            PortUsage::InUse { holders }
        }
    }

    pub fn holders(&self) -> &[PortHolder] {
        match self {
            PortUsage::InUse { holders } => holders,
            _ => &[],
        }
    }

    pub fn is_free(&self) -> bool {
        matches!(self, PortUsage::Free)
    }

    pub fn is_in_use(&self) -> bool {
        matches!(self, PortUsage::InUse { .. })
    }

    /// "VARA FM (VARAFM.exe, PID 4312)" or "free".
    pub fn describe(&self) -> String {
        match self {
            PortUsage::Free => "free".into(),
            PortUsage::InUse { holders } => holders
                .iter()
                .map(PortHolder::describe)
                .collect::<Vec<_>>()
                .join(", "),
            PortUsage::Unknown { reason } => format!("unknown ({reason})"),
        }
    }

    /// Names of the programs holding the port, for short messages:
    /// "VARA FM" or "VARA FM and WSJT-X".
    pub fn holder_names(&self) -> String {
        let names: Vec<String> = self
            .holders()
            .iter()
            .map(PortHolder::display_name)
            .collect();
        match names.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }
}

/// The result of checking a set of ports at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    /// Keyed by the port name applications use (`COM7`, `/dev/ttyUSB0`).
    pub ports: BTreeMap<String, PortUsage>,
    /// Programs this check cannot see, e.g. ones running as administrator
    /// while ComInspect is not.
    pub limitation: Option<String>,
    /// How long the check took.
    pub duration_ms: u64,
}

/// A port whose usage differs between two snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageChange {
    pub port: String,
    /// `None` when the port was not checked before.
    pub before: Option<PortUsage>,
    pub after: PortUsage,
}

impl UsageChange {
    /// True when programs were holding the port and now none is.
    pub fn released(&self) -> bool {
        matches!(&self.before, Some(PortUsage::InUse { .. })) && self.after.is_free()
    }
}

/// Ports whose usage changed. A failed check ([`PortUsage::Unknown`]) never
/// counts as a release.
pub fn changes(old: &UsageSnapshot, new: &UsageSnapshot) -> Vec<UsageChange> {
    new.ports
        .iter()
        .filter(|(port, after)| old.ports.get(*port) != Some(*after))
        .map(|(port, after)| UsageChange {
            port: port.clone(),
            before: old.ports.get(port).cloned(),
            after: after.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder(pid: u32, name: &str, description: Option<&str>) -> PortHolder {
        PortHolder {
            pid,
            process_name: name.into(),
            executable: None,
            description: description.map(Into::into),
        }
    }

    #[test]
    fn display_names() {
        assert_eq!(
            holder(1, "VARAFM.exe", Some("VARA FM")).display_name(),
            "VARA FM"
        );
        assert_eq!(holder(1, "VarAC.EXE", None).display_name(), "VarAC");
        assert_eq!(holder(1, "wsjtx", Some("  ")).display_name(), "wsjtx");
        assert_eq!(
            holder(4312, "VARAFM.exe", Some("VARA FM")).describe(),
            "VARA FM (VARAFM.exe, PID 4312)"
        );
        assert_eq!(holder(7, "flrig", None).describe(), "flrig (PID 7)");
    }

    #[test]
    fn usage_normalizes_holders() {
        assert_eq!(PortUsage::in_use(vec![]), PortUsage::Free);
        let usage = PortUsage::in_use(vec![
            holder(9, "b.exe", None),
            holder(3, "a.exe", None),
            holder(9, "b.exe", None),
        ]);
        assert_eq!(usage.holders().len(), 2);
        assert_eq!(usage.holders()[0].pid, 3);
        assert_eq!(usage.holder_names(), "a and b");
        let three = PortUsage::in_use(vec![
            holder(1, "x.exe", None),
            holder(2, "y.exe", None),
            holder(3, "z.exe", None),
        ]);
        assert_eq!(three.holder_names(), "x, y and z");
    }

    #[test]
    fn detects_releases_but_not_failed_checks() {
        let vara = PortUsage::in_use(vec![holder(4312, "VARAFM.exe", Some("VARA FM"))]);
        let mut old = UsageSnapshot::default();
        old.ports.insert("COM4".into(), vara.clone());
        old.ports.insert("COM5".into(), vara.clone());
        old.ports.insert("COM6".into(), PortUsage::Free);

        let mut new = UsageSnapshot::default();
        new.ports.insert("COM4".into(), PortUsage::Free);
        new.ports.insert(
            "COM5".into(),
            PortUsage::Unknown {
                reason: "check failed".into(),
            },
        );
        new.ports.insert("COM6".into(), PortUsage::Free);
        new.ports.insert("COM7".into(), vara.clone());

        let changes = changes(&old, &new);
        let ports: Vec<_> = changes.iter().map(|c| c.port.as_str()).collect();
        assert_eq!(ports, ["COM4", "COM5", "COM7"]);
        assert!(changes[0].released());
        assert!(!changes[1].released(), "an unknown result is not a release");
        assert!(!changes[2].released());
        assert!(changes[2].before.is_none());
    }

    #[test]
    fn serializes_with_a_state_tag() {
        let usage = PortUsage::in_use(vec![holder(1, "VarAC.exe", None)]);
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["state"], "in_use");
        assert_eq!(json["holders"][0]["processName"], "VarAC.exe");
        assert_eq!(
            serde_json::to_value(PortUsage::Free).unwrap()["state"],
            "free"
        );
    }
}
