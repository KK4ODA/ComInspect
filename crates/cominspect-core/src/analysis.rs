//! Conflict and health analysis of a scan.
//!
//! Produces findings such as duplicate COM assignments, stale COM-number
//! reservations, driver problems and permission issues. Findings are purely
//! informational: nothing here changes the system.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::identity::{is_unique_serial, normalize_serial};
use crate::model::{Presence, ScanResult, short_port_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Shown in the inspector only.
    Info,
    /// Marks the row and is counted in the status bar.
    Warning,
    /// The device is present but not usable.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable identifier of the finding type (e.g. `duplicate-port`).
    pub code: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub port_name: Option<String>,
    /// Indices into [`ScanResult::ports`] this finding relates to. Empty for
    /// system-wide findings.
    pub ports: Vec<usize>,
}

impl Finding {
    fn new(code: &str, severity: Severity, title: String, detail: String) -> Self {
        Finding {
            code: code.to_string(),
            severity,
            title,
            detail,
            port_name: None,
            ports: Vec::new(),
        }
    }

    fn for_ports(mut self, port_name: Option<String>, ports: Vec<usize>) -> Self {
        self.port_name = port_name;
        self.ports = ports;
        self
    }
}

/// Text of a Windows Device Manager problem code (`CM_PROB_*`).
pub fn windows_problem_description(code: u32) -> String {
    let text = match code {
        1 => "This device is not configured correctly.",
        3 => "The driver for this device might be corrupted, or the system may be low on memory.",
        10 => "This device cannot start.",
        12 => "This device cannot find enough free resources that it can use.",
        14 => "This device cannot work properly until you restart your computer.",
        18 => "Reinstall the drivers for this device.",
        19 => {
            "Windows cannot start this device because its configuration information is incomplete or damaged."
        }
        21 => "Windows is removing this device.",
        22 => "This device is disabled.",
        24 => {
            "This device is not present, is not working properly, or does not have all its drivers installed."
        }
        28 => "The drivers for this device are not installed.",
        29 => {
            "This device is disabled because the firmware of the device did not give it the required resources."
        }
        31 => "Windows cannot load the drivers required for this device.",
        32 => "A driver (service) for this device has been disabled.",
        37 => "Windows cannot initialize the device driver for this hardware.",
        39 => {
            "Windows cannot load the device driver for this hardware. The driver may be corrupted or missing."
        }
        43 => "Windows has stopped this device because it has reported problems.",
        45 => "This hardware device is not connected to the computer.",
        48 => {
            "The software for this device has been blocked from starting because it is known to have problems with Windows."
        }
        52 => {
            "Windows cannot verify the digital signature for the drivers required for this device."
        }
        _ => "This device has a problem.",
    };
    format!("{text} (Code {code})")
}

/// Advice for a problem code, taking the device's USB vendor into account.
fn problem_advice(code: u32, vid: Option<u16>) -> Option<&'static str> {
    match (code, vid) {
        (10, Some(0x067B)) => Some(
            "The Prolific driver refused to start this chip. This almost always means a counterfeit \
             or discontinued PL2303 chip, which is very common in inexpensive radio programming \
             cables. Options: install an older Prolific driver that still supports the chip, or \
             replace the cable with an FTDI- or CP210x-based one.",
        ),
        (28, Some(0x1A86)) => Some(
            "Install the WCH CH340/CH341 driver (available from the chip vendor or through Windows \
             Update), then reconnect the device.",
        ),
        (28, _) => Some(
            "Install the driver supplied by the device or chip vendor, then reconnect the device.",
        ),
        (22, _) => Some("Enable the device in Device Manager (right-click → Enable device)."),
        (43, _) => Some(
            "The device reported a failure. Try a different USB cable or socket, avoid unpowered \
             hubs, and check for RF getting into the USB cable (ferrite chokes help).",
        ),
        (52, _) => Some("Install a signed driver from the device vendor."),
        (10, _) => {
            Some("Reconnect the device. If the problem persists, reinstall or update its driver.")
        }
        _ => None,
    }
}

/// Analyzes one scan.
pub fn analyze_scan(scan: &ScanResult) -> Vec<Finding> {
    let mut findings = Vec::new();
    let is_windows = scan.platform == "windows";

    // --- Duplicate port assignments -------------------------------------
    let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, port) in scan.ports.iter().enumerate() {
        by_name
            .entry(port.port_name.trim().to_uppercase())
            .or_default()
            .push(i);
    }
    for indices in by_name.values().filter(|v| v.len() > 1) {
        let name = scan.ports[indices[0]].port_name.clone();
        let present = indices
            .iter()
            .filter(|&&i| scan.ports[i].presence == Presence::Present)
            .count();
        let (severity, title) = match present {
            0 => (
                Severity::Info,
                format!(
                    "{name} is assigned to {} disconnected devices",
                    indices.len()
                ),
            ),
            1 => (
                Severity::Warning,
                format!("{name} is also assigned to a disconnected device"),
            ),
            n => (
                Severity::Warning,
                format!("{name} is assigned to {n} connected devices"),
            ),
        };
        let detail = if present > 1 {
            "Two connected devices claim the same port name. Only one of them can be opened; \
             change one of them to a free COM number."
                .to_string()
        } else {
            "When the disconnected device is plugged in again the two will conflict and one of \
             them may not be usable. Removing the unused hidden device frees the number."
                .to_string()
        };
        findings.push(
            Finding::new("duplicate-port", severity, title, detail)
                .for_ports(Some(name), indices.clone()),
        );
    }

    // --- Driver problems ------------------------------------------------
    for (i, port) in scan.ports.iter().enumerate() {
        if port.presence != Presence::Present {
            continue;
        }
        if let Some(problem) = &port.system.problem {
            let vid = port.usb.as_ref().map(|u| u.vid);
            let mut detail = problem.description.clone();
            if let Some(advice) = problem_advice(problem.code, vid) {
                detail.push(' ');
                detail.push_str(advice);
            }
            findings.push(
                Finding::new(
                    "device-problem",
                    Severity::Error,
                    format!("{} has a driver problem", port.short_port_name()),
                    detail,
                )
                .for_ports(Some(port.port_name.clone()), vec![i]),
            );
        }
    }

    // --- USB serial numbers -------------------------------------------
    let mut by_serial: HashMap<(u16, u16, String, u8), Vec<usize>> = HashMap::new();
    for (i, port) in scan.ports.iter().enumerate() {
        if port.presence != Presence::Present {
            continue;
        }
        let Some(usb) = &port.usb else { continue };
        let Some(serial) = usb.serial_number.as_deref().map(normalize_serial) else {
            continue;
        };
        if serial.is_empty() {
            continue;
        }
        if !is_unique_serial(&serial) {
            findings.push(
                Finding::new(
                    "generic-serial",
                    Severity::Info,
                    format!("Serial number {serial} is not unique"),
                    format!(
                        "{serial} is a default value shared by many devices of this type, so \
                         ComInspect recognizes this device by the USB socket it is plugged into. \
                         Keep it in the same socket, or use \"Same device as…\" after moving it."
                    ),
                )
                .for_ports(Some(port.port_name.clone()), vec![i]),
            );
        }
        by_serial
            .entry((usb.vid, usb.pid, serial, usb.interface_number.unwrap_or(0)))
            .or_default()
            .push(i);
    }
    let mut duplicate_groups: Vec<_> = by_serial.into_iter().filter(|(_, v)| v.len() > 1).collect();
    duplicate_groups.sort_by(|a, b| a.1.cmp(&b.1));
    for ((_, _, serial, _), indices) in duplicate_groups {
        let names: Vec<_> = indices
            .iter()
            .map(|&i| short_port_name(&scan.ports[i].port_name).to_string())
            .collect();
        findings.push(
            Finding::new(
                "duplicate-serial",
                Severity::Warning,
                format!("{} report the same USB serial number", names.join(" and ")),
                format!(
                    "Several connected devices report the serial number {serial}. They can only be \
                     told apart by the USB socket they are plugged into, so keep each one in its \
                     own socket."
                ),
            )
            .for_ports(None, indices),
        );
    }

    // --- Unix permissions -----------------------------------------------
    for (i, port) in scan.ports.iter().enumerate() {
        let Some(access) = &port.system.access else {
            continue;
        };
        if port.presence != Presence::Present || (access.readable && access.writable) {
            continue;
        }
        let group = access.owner_group.as_deref().unwrap_or("dialout");
        findings.push(
            Finding::new(
                "no-permission",
                Severity::Warning,
                format!("No permission to open {}", port.short_port_name()),
                format!(
                    "Your user account cannot open {}. Add yourself to the '{group}' group \
                     (for example: sudo usermod -aG {group} $USER), then log out and back in.",
                    port.port_name
                ),
            )
            .for_ports(Some(port.port_name.clone()), vec![i]),
        );
    }

    // --- COM number reservations (Windows) --------------------------------
    if is_windows {
        let claimed: BTreeSet<u32> = scan.ports.iter().filter_map(|p| p.com_number()).collect();
        let held_by_absent: BTreeSet<u32> = scan
            .ports
            .iter()
            .filter(|p| p.presence == Presence::Absent)
            .filter_map(|p| p.com_number())
            .collect();
        let present_numbers: BTreeSet<u32> = scan
            .ports
            .iter()
            .filter(|p| p.presence == Presence::Present)
            .filter_map(|p| p.com_number())
            .collect();
        let held_only_by_absent: BTreeSet<u32> = held_by_absent
            .difference(&present_numbers)
            .copied()
            .collect();
        let stale: BTreeSet<u32> = scan
            .reserved
            .as_ref()
            .map(|r| {
                r.numbers
                    .iter()
                    .copied()
                    .filter(|n| !claimed.contains(n))
                    .collect()
            })
            .unwrap_or_default();

        if !stale.is_empty() {
            let list = stale.iter().map(|n| format!("COM{n}")).collect::<Vec<_>>();
            findings.push(Finding::new(
                "stale-reservation",
                Severity::Info,
                format!(
                    "{} COM number{} reserved without a device",
                    stale.len(),
                    if stale.len() == 1 { " is" } else { "s are" }
                ),
                format!(
                    "{} {} marked as in use in the Windows COM port database, but no device \
                     (connected or hidden) claims {}. These reservations are left behind by \
                     uninstalled devices and push new devices to higher numbers.",
                    list.join(", "),
                    if list.len() == 1 { "is" } else { "are" },
                    if list.len() == 1 { "it" } else { "them" }
                ),
            ));
        }
        if !held_only_by_absent.is_empty() {
            findings.push(Finding::new(
                "absent-reservations",
                Severity::Info,
                format!(
                    "{} COM number{} held by disconnected devices",
                    held_only_by_absent.len(),
                    if held_only_by_absent.len() == 1 {
                        " is"
                    } else {
                        "s are"
                    }
                ),
                "Windows keeps a COM number reserved for every serial device it has ever seen, \
                 even when the device is unplugged. That is why new devices receive ever higher \
                 numbers. Removing hidden devices you no longer use (Device Manager → View → Show \
                 hidden devices) frees their numbers."
                    .to_string(),
            ));
        }
        for (i, port) in scan.ports.iter().enumerate() {
            if port.presence != Presence::Present {
                continue;
            }
            let Some(n) = port.com_number() else { continue };
            if n < 10 {
                continue;
            }
            let lower_blocked = held_only_by_absent
                .iter()
                .chain(stale.iter())
                .filter(|&&m| m < n)
                .count();
            let mut detail = String::from(
                "Some older programs (including some radio programming software) can only open \
                 COM1–COM9, or list only COM1–COM16.",
            );
            if lower_blocked > 0 {
                detail.push_str(&format!(
                    " {lower_blocked} lower number{} reserved by disconnected devices or stale \
                     reservations, which is why this port received a high number.",
                    if lower_blocked == 1 { " is" } else { "s are" }
                ));
            }
            findings.push(
                Finding::new(
                    "high-port-number",
                    Severity::Info,
                    format!("COM{n} is a high port number"),
                    detail,
                )
                .for_ports(Some(port.port_name.clone()), vec![i]),
            );
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AccessInfo, DeviceProblem, DiscoveredPort, ReservedPorts, SystemInfo, UsbInfo,
    };

    fn port(name: &str, presence: Presence) -> DiscoveredPort {
        DiscoveredPort {
            port_name: name.into(),
            presence,
            ..Default::default()
        }
    }

    fn windows_scan(ports: Vec<DiscoveredPort>, reserved: &[u32]) -> ScanResult {
        ScanResult {
            platform: "windows".into(),
            ports,
            reserved: Some(ReservedPorts {
                numbers: reserved.to_vec(),
                database_size: 256,
                source: "test".into(),
            }),
            ..Default::default()
        }
    }

    fn codes(findings: &[Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.code.as_str()).collect()
    }

    #[test]
    fn detects_duplicate_assignment_with_hidden_device() {
        let scan = windows_scan(
            vec![
                port("COM7", Presence::Present),
                port("COM7", Presence::Absent),
            ],
            &[7],
        );
        let findings = analyze_scan(&scan);
        let dup = findings
            .iter()
            .find(|f| f.code == "duplicate-port")
            .unwrap();
        assert_eq!(dup.severity, Severity::Warning);
        assert_eq!(dup.ports, vec![0, 1]);
        assert!(dup.title.contains("disconnected"));
    }

    #[test]
    fn stale_and_absent_reservations_explain_high_numbers() {
        let scan = windows_scan(
            vec![
                port("COM3", Presence::Absent),
                port("COM4", Presence::Absent),
                port("COM23", Presence::Present),
            ],
            &[1, 3, 4, 9, 23],
        );
        let findings = analyze_scan(&scan);
        assert_eq!(
            codes(&findings),
            vec![
                "stale-reservation",
                "absent-reservations",
                "high-port-number"
            ]
        );
        assert!(findings[0].detail.starts_with("COM1, COM9 are"));
        assert!(findings[2].detail.contains("4 lower numbers"));
        assert_eq!(findings[2].ports, vec![2]);
    }

    #[test]
    fn no_reservation_findings_outside_windows() {
        let mut scan = windows_scan(vec![port("/dev/ttyUSB10", Presence::Present)], &[3]);
        scan.platform = "linux".into();
        assert!(analyze_scan(&scan).is_empty());
    }

    #[test]
    fn prolific_code_10_gets_specific_advice() {
        let mut p = port("COM5", Presence::Present);
        p.usb = Some(UsbInfo {
            vid: 0x067B,
            pid: 0x2303,
            ..Default::default()
        });
        p.system = SystemInfo {
            problem: Some(DeviceProblem {
                code: 10,
                description: windows_problem_description(10),
            }),
            ..Default::default()
        };
        let findings = analyze_scan(&windows_scan(vec![p], &[5]));
        let problem = &findings[0];
        assert_eq!(problem.severity, Severity::Error);
        assert!(problem.detail.contains("(Code 10)"));
        assert!(problem.detail.contains("counterfeit"));
    }

    #[test]
    fn duplicate_and_generic_serials() {
        let mk = |name: &str, serial: &str| {
            let mut p = port(name, Presence::Present);
            p.usb = Some(UsbInfo {
                vid: 0x10C4,
                pid: 0xEA60,
                serial_number: Some(serial.into()),
                ..Default::default()
            });
            p
        };
        let scan = windows_scan(vec![mk("COM3", "0001"), mk("COM4", "0001")], &[3, 4]);
        let findings = analyze_scan(&scan);
        assert_eq!(
            codes(&findings),
            vec!["generic-serial", "generic-serial", "duplicate-serial"]
        );
        assert_eq!(
            findings[2].title,
            "COM3 and COM4 report the same USB serial number"
        );
    }

    #[test]
    fn permission_problem_on_linux() {
        let mut p = port("/dev/ttyUSB0", Presence::Present);
        p.system.access = Some(AccessInfo {
            readable: false,
            writable: false,
            owner_group: Some("dialout".into()),
            mode: Some("crw-rw----".into()),
        });
        let scan = ScanResult {
            platform: "linux".into(),
            ports: vec![p],
            ..Default::default()
        };
        let findings = analyze_scan(&scan);
        assert_eq!(codes(&findings), vec!["no-permission"]);
        assert!(findings[0].detail.contains("usermod -aG dialout"));
    }
}
