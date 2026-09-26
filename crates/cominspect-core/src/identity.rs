//! Identity engine.
//!
//! Derives *identity keys* (a fingerprint) from each discovered port and
//! matches them against previously known devices. The COM number is never
//! the identity: it is only used as a last-resort hint for ports that expose
//! nothing better (see `docs/ARCHITECTURE.md` §6).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{DiscoveredPort, InstanceQuality, Transport};

/// Kind of identity evidence, strongest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyKind {
    /// `VID:PID:SERIAL:ifN` — a vendor-programmed unique USB serial number.
    UsbSerial,
    /// Remote Bluetooth device address (+ optional service qualifier).
    Bluetooth,
    /// OS device instance identifier.
    OsDevice,
    /// `VID:PID@LOCATION:ifN` — the physical USB socket.
    UsbPath,
    /// `provider:PORTNAME` for software ports.
    Virtual,
}

impl KeyKind {
    pub const ALL: [KeyKind; 5] = [
        KeyKind::UsbSerial,
        KeyKind::Bluetooth,
        KeyKind::OsDevice,
        KeyKind::UsbPath,
        KeyKind::Virtual,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            KeyKind::UsbSerial => "usb-serial",
            KeyKind::Bluetooth => "bluetooth",
            KeyKind::OsDevice => "os-device",
            KeyKind::UsbPath => "usb-path",
            KeyKind::Virtual => "virtual",
        }
    }

    pub fn parse(value: &str) -> Option<KeyKind> {
        KeyKind::ALL.into_iter().find(|k| k.as_str() == value)
    }

    /// Whether the key identifies the same device on a different computer.
    pub fn portable(self) -> bool {
        matches!(
            self,
            KeyKind::UsbSerial | KeyKind::Bluetooth | KeyKind::Virtual
        )
    }

    pub fn description(self) -> &'static str {
        match self {
            KeyKind::UsbSerial => "USB serial number",
            KeyKind::Bluetooth => "Bluetooth device address",
            KeyKind::OsDevice => "operating-system device instance",
            KeyKind::UsbPath => "USB port location",
            KeyKind::Virtual => "virtual-port provider and name",
        }
    }
}

/// Strength of each kind of evidence (0–100).
pub mod strength {
    pub const USB_SERIAL: u8 = 100;
    pub const BLUETOOTH: u8 = 95;
    pub const OS_DEVICE_UNIQUE: u8 = 90;
    pub const OS_DEVICE_STABLE: u8 = 80;
    pub const OS_DEVICE_LOCATION: u8 = 70;
    pub const USB_PATH: u8 = 60;
    pub const VIRTUAL: u8 = 50;
    pub const PORT_NAME: u8 = 10;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityKey {
    pub kind: KeyKind,
    pub value: String,
    pub strength: u8,
}

impl IdentityKey {
    pub fn new(kind: KeyKind, value: impl Into<String>, strength: u8) -> Self {
        IdentityKey {
            kind,
            value: value.into(),
            strength,
        }
    }
}

/// Serial numbers that many devices share and therefore identify nothing.
const NON_UNIQUE_SERIALS: &[&str] = &[
    "0001",
    "00000001",
    "0123456789",
    "123456789",
    "1234567890",
    "12345678",
    "123456",
    "ABCDEF",
    "A50285BI",
    "FFFFFFFF",
    "NONE",
    "N/A",
    "NA",
    "NULL",
    "UNKNOWN",
    "SERIAL",
    "SERIALNUMBER",
    "SERIAL_NUMBER",
    "DEFAULT",
];

/// Canonical form of a USB serial number: trimmed, upper-case, internal
/// whitespace replaced by `_` (Windows replaces spaces in device IDs, so this
/// makes Windows, Linux and macOS agree).
pub fn normalize_serial(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
        .to_uppercase()
}

/// Whether a (normalized) serial number is plausibly unique to one device.
pub fn is_unique_serial(normalized: &str) -> bool {
    if normalized.chars().count() < 3 {
        return false;
    }
    if normalized.trim_start_matches('0').chars().count() < 2 {
        return false;
    }
    let mut chars = normalized.chars();
    let first = chars.next().unwrap_or_default();
    if chars.all(|c| c == first) {
        return false;
    }
    !NON_UNIQUE_SERIALS.contains(&normalized)
}

/// Returns the normalized serial if it can serve as a strong identity.
pub fn usable_serial(raw: &str) -> Option<String> {
    let normalized = normalize_serial(raw);
    is_unique_serial(&normalized).then_some(normalized)
}

/// Canonical `AA:BB:CC:DD:EE:FF` form of a Bluetooth address. Returns `None`
/// for malformed and all-zero addresses (incoming ports).
pub fn normalize_bt_address(raw: &str) -> Option<String> {
    let hex: String = raw
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if hex.len() != 12 || hex.chars().all(|c| c == '0') {
        return None;
    }
    let pairs: Vec<&str> = (0..6).map(|i| &hex[i * 2..i * 2 + 2]).collect();
    Some(pairs.join(":"))
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Derives the identity keys of a port, strongest first.
pub fn derive_keys(port: &DiscoveredPort) -> Vec<IdentityKey> {
    let mut keys = Vec::new();

    if let Some(usb) = &port.usb {
        let iface = usb.interface_number.unwrap_or(0);
        if let Some(serial) = usb.serial_number.as_deref().and_then(usable_serial) {
            keys.push(IdentityKey::new(
                KeyKind::UsbSerial,
                format!("{:04X}:{:04X}:{}:if{}", usb.vid, usb.pid, serial, iface),
                strength::USB_SERIAL,
            ));
        }
        if let Some(location) = usb.location.as_deref().map(str::trim)
            && !location.is_empty()
        {
            keys.push(IdentityKey::new(
                KeyKind::UsbPath,
                format!("{:04X}:{:04X}@{}:if{}", usb.vid, usb.pid, location, iface),
                strength::USB_PATH,
            ));
        }
    }

    if let Some(bt) = &port.bluetooth
        && let Some(address) = bt.address.as_deref().and_then(normalize_bt_address)
    {
        let value = match bt.service_key.as_deref().map(str::trim) {
            Some(q) if !q.is_empty() => format!("{address}#{}", q.to_uppercase()),
            _ => address,
        };
        keys.push(IdentityKey::new(
            KeyKind::Bluetooth,
            value,
            strength::BLUETOOTH,
        ));
    }

    if let Some(instance) = &port.instance_identity {
        let value = instance.value.trim();
        if !value.is_empty() {
            let s = match instance.quality {
                InstanceQuality::DeviceUnique => strength::OS_DEVICE_UNIQUE,
                InstanceQuality::Stable => strength::OS_DEVICE_STABLE,
                InstanceQuality::Location => strength::OS_DEVICE_LOCATION,
            };
            keys.push(IdentityKey::new(KeyKind::OsDevice, value.to_uppercase(), s));
        }
    }

    if port.transport == Transport::Virtual {
        let provider = port
            .virtual_port
            .as_ref()
            .map(|v| slug(&v.provider))
            .unwrap_or_else(|| "unknown".to_string());
        keys.push(IdentityKey::new(
            KeyKind::Virtual,
            format!("{provider}:{}", port.port_name.trim().to_uppercase()),
            strength::VIRTUAL,
        ));
    }

    keys.sort_by(|a, b| b.strength.cmp(&a.strength).then(a.kind.cmp(&b.kind)));
    keys.dedup_by(|a, b| a.kind == b.kind && a.value == b.value);
    keys
}

/// Attributes used to veto matches when stronger evidence disagrees.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAttrs {
    pub transport: Transport,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    /// Normalized serial, even when it is not unique.
    pub serial: Option<String>,
    pub interface: Option<u8>,
    pub bt_address: Option<String>,
}

impl DeviceAttrs {
    pub fn from_port(port: &DiscoveredPort) -> Self {
        let usb = port.usb.as_ref();
        DeviceAttrs {
            transport: port.transport,
            vid: usb.map(|u| u.vid),
            pid: usb.map(|u| u.pid),
            serial: usb
                .and_then(|u| u.serial_number.as_deref())
                .map(normalize_serial)
                .filter(|s| !s.is_empty()),
            interface: usb.and_then(|u| u.interface_number),
            bt_address: port
                .bluetooth
                .as_ref()
                .and_then(|b| b.address.as_deref())
                .and_then(normalize_bt_address),
        }
    }

    /// `false` when the two sets of attributes prove different devices.
    /// Weak matches (USB socket, virtual name, port name) additionally require
    /// both sides to agree on whether a serial number exists.
    pub fn compatible_with(&self, other: &DeviceAttrs, match_strength: u8) -> bool {
        let weak = match_strength <= strength::USB_PATH;
        if self.transport != Transport::Unknown
            && other.transport != Transport::Unknown
            && self.transport != other.transport
        {
            return false;
        }
        if differ(&self.vid, &other.vid) || differ(&self.pid, &other.pid) {
            return false;
        }
        match (&self.serial, &other.serial) {
            (Some(a), Some(b)) if a != b => return false,
            (Some(_), None) | (None, Some(_)) if weak => return false,
            _ => {}
        }
        if differ(&self.interface, &other.interface) || differ(&self.bt_address, &other.bt_address)
        {
            return false;
        }
        true
    }
}

fn differ<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x != y)
}

/// A stored key together with when it was last observed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownKey {
    pub key: IdentityKey,
    pub last_seen: i64,
}

/// What the engine needs to know about a previously seen device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownDevice {
    pub id: i64,
    pub keys: Vec<KnownKey>,
    pub attrs: DeviceAttrs,
    pub last_port: Option<String>,
    pub last_observed: i64,
}

/// Why a port was recognized as a known device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchBasis {
    UsbSerial,
    Bluetooth,
    OsDevice,
    UsbPath,
    Virtual,
    PortName,
}

impl MatchBasis {
    fn from_kind(kind: KeyKind) -> Self {
        match kind {
            KeyKind::UsbSerial => MatchBasis::UsbSerial,
            KeyKind::Bluetooth => MatchBasis::Bluetooth,
            KeyKind::OsDevice => MatchBasis::OsDevice,
            KeyKind::UsbPath => MatchBasis::UsbPath,
            KeyKind::Virtual => MatchBasis::Virtual,
        }
    }

    /// Sentence fragment for the inspector ("Recognized by …").
    pub fn description(self) -> &'static str {
        match self {
            MatchBasis::UsbSerial => "USB serial number",
            MatchBasis::Bluetooth => "Bluetooth device address",
            MatchBasis::OsDevice => "operating-system device instance",
            MatchBasis::UsbPath => "USB port location",
            MatchBasis::Virtual => "virtual-port provider and name",
            MatchBasis::PortName => "port name only",
        }
    }
}

/// Result of matching one discovered port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortMatch {
    /// Keys to associate with the device (strongest first).
    pub keys: Vec<IdentityKey>,
    /// The matched device, or `None` if this is a new device.
    pub device_id: Option<i64>,
    pub basis: Option<MatchBasis>,
    /// The port's USB serial number is shared with another port in this scan
    /// and was therefore not used as an identity.
    pub ambiguous_serial: bool,
}

struct Edge {
    port: usize,
    device: usize,
    strength: u8,
    recency: i64,
    basis: MatchBasis,
}

/// Matches every discovered port to at most one known device (and vice
/// versa). Ports without a match are new devices.
pub fn match_ports(ports: &[DiscoveredPort], known: &[KnownDevice]) -> Vec<PortMatch> {
    let mut prepared: Vec<(Vec<IdentityKey>, DeviceAttrs, bool)> = ports
        .iter()
        .map(|p| (derive_keys(p), DeviceAttrs::from_port(p), false))
        .collect();

    // A USB serial number shared by several ports in the same scan is not
    // unique, whatever it looks like: fall back to weaker keys for those ports.
    let mut serial_counts: HashMap<String, usize> = HashMap::new();
    for (keys, _, _) in &prepared {
        for key in keys.iter().filter(|k| k.kind == KeyKind::UsbSerial) {
            *serial_counts.entry(key.value.clone()).or_default() += 1;
        }
    }
    for (keys, _, ambiguous) in &mut prepared {
        let before = keys.len();
        keys.retain(|k| k.kind != KeyKind::UsbSerial || serial_counts[&k.value] < 2);
        *ambiguous = keys.len() != before;
    }

    let mut index: HashMap<(KeyKind, &str), Vec<(usize, i64)>> = HashMap::new();
    for (di, device) in known.iter().enumerate() {
        for k in &device.keys {
            index
                .entry((k.key.kind, k.key.value.as_str()))
                .or_default()
                .push((di, k.last_seen));
        }
    }

    let mut edges = Vec::new();
    for (pi, (keys, attrs, _)) in prepared.iter().enumerate() {
        let mut found = false;
        for key in keys {
            let Some(candidates) = index.get(&(key.kind, key.value.as_str())) else {
                continue;
            };
            for &(di, key_seen) in candidates {
                if attrs.compatible_with(&known[di].attrs, key.strength) {
                    found = true;
                    edges.push(Edge {
                        port: pi,
                        device: di,
                        strength: key.strength,
                        // When several ports/devices compete for the same key,
                        // prefer the pairing that was observed most recently.
                        recency: key_seen,
                        basis: MatchBasis::from_kind(key.kind),
                    });
                }
            }
        }
        if found {
            continue;
        }
        // Last resort: same port name, for devices that have no identity
        // evidence at all (typically macOS Bluetooth/system ports).
        let port_name = ports[pi].port_name.trim();
        for (di, device) in known.iter().enumerate() {
            let same_name = device
                .last_port
                .as_deref()
                .is_some_and(|p| p.trim().eq_ignore_ascii_case(port_name));
            let has_real_identity = device
                .keys
                .iter()
                .any(|k| k.key.strength >= strength::USB_PATH);
            if same_name
                && !has_real_identity
                && keys.iter().all(|k| k.strength < strength::USB_PATH)
                && attrs.compatible_with(&device.attrs, strength::PORT_NAME)
                && attrs.transport == device.attrs.transport
            {
                edges.push(Edge {
                    port: pi,
                    device: di,
                    strength: strength::PORT_NAME,
                    recency: device.last_observed,
                    basis: MatchBasis::PortName,
                });
            }
        }
    }

    edges.sort_by(|a, b| {
        b.strength
            .cmp(&a.strength)
            .then(b.recency.cmp(&a.recency))
            .then(a.port.cmp(&b.port))
            .then(a.device.cmp(&b.device))
    });

    let mut assigned: Vec<Option<(usize, MatchBasis)>> = vec![None; ports.len()];
    let mut used_devices: HashSet<usize> = HashSet::new();
    for edge in edges {
        if assigned[edge.port].is_some() || used_devices.contains(&edge.device) {
            continue;
        }
        assigned[edge.port] = Some((edge.device, edge.basis));
        used_devices.insert(edge.device);
    }

    prepared
        .into_iter()
        .zip(assigned)
        .map(|((keys, _, ambiguous_serial), assignment)| PortMatch {
            keys,
            device_id: assignment.map(|(di, _)| known[di].id),
            basis: assignment.map(|(_, basis)| basis),
            ambiguous_serial,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BluetoothInfo, InstanceIdentity, Presence, UsbInfo, VirtualInfo};

    fn usb_port(
        name: &str,
        vid: u16,
        pid: u16,
        serial: Option<&str>,
        iface: Option<u8>,
        location: &str,
    ) -> DiscoveredPort {
        DiscoveredPort {
            port_name: name.into(),
            transport: Transport::Usb,
            usb: Some(UsbInfo {
                vid,
                pid,
                serial_number: serial.map(Into::into),
                interface_number: iface,
                location: Some(location.into()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Simulates the store: turns matched/new ports into known devices.
    fn remember(ports: &[DiscoveredPort], known: &mut Vec<KnownDevice>, now: i64) -> Vec<i64> {
        let matches = match_ports(ports, known);
        let mut ids = Vec::new();
        for (port, m) in ports.iter().zip(matches) {
            let id = match m.device_id {
                Some(id) => id,
                None => {
                    let id = known.len() as i64 + 1;
                    known.push(KnownDevice {
                        id,
                        attrs: DeviceAttrs::from_port(port),
                        ..Default::default()
                    });
                    id
                }
            };
            let device = known.iter_mut().find(|d| d.id == id).unwrap();
            for key in m.keys {
                if !device.keys.iter().any(|k| k.key == key) {
                    device.keys.push(KnownKey {
                        key,
                        last_seen: now,
                    });
                }
            }
            device.last_port = Some(port.port_name.clone());
            device.last_observed = now;
            ids.push(id);
        }
        ids
    }

    #[test]
    fn serial_hygiene() {
        assert_eq!(normalize_serial(" IC-7300 03001234 "), "IC-7300_03001234");
        assert!(usable_serial("A10XYZ12").is_some());
        assert!(usable_serial("IC-7300 03001234").is_some());
        for bad in [
            "0001",
            "0",
            "01",
            "0000000",
            "AAAA",
            "A50285BI",
            "  ",
            "1234567890",
        ] {
            assert_eq!(usable_serial(bad), None, "{bad} should not be unique");
        }
    }

    #[test]
    fn bluetooth_addresses_are_canonical() {
        assert_eq!(
            normalize_bt_address("001122aabbcc").as_deref(),
            Some("00:11:22:AA:BB:CC")
        );
        assert_eq!(
            normalize_bt_address("00:11:22:aa:bb:cc").as_deref(),
            Some("00:11:22:AA:BB:CC")
        );
        assert_eq!(normalize_bt_address("000000000000"), None);
        assert_eq!(normalize_bt_address("0011"), None);
    }

    #[test]
    fn derives_keys_for_multi_port_usb_device() {
        let enhanced = usb_port("COM7", 0x10C4, 0xEA70, Some("01A2B3C4"), Some(0), "LOC1");
        let standard = usb_port("COM8", 0x10C4, 0xEA70, Some("01A2B3C4"), Some(1), "LOC1");
        let ek = derive_keys(&enhanced);
        let sk = derive_keys(&standard);
        assert_eq!(ek[0].kind, KeyKind::UsbSerial);
        assert_eq!(ek[0].value, "10C4:EA70:01A2B3C4:if0");
        assert_eq!(sk[0].value, "10C4:EA70:01A2B3C4:if1");
        assert_eq!(ek[1].value, "10C4:EA70@LOC1:if0");
        let matches = match_ports(&[enhanced, standard], &[]);
        assert!(
            matches
                .iter()
                .all(|m| m.device_id.is_none() && !m.ambiguous_serial)
        );
    }

    #[test]
    fn generic_serial_only_yields_location_key() {
        let port = usb_port("COM3", 0x10C4, 0xEA60, Some("0001"), None, "LOC2");
        let keys = derive_keys(&port);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].kind, KeyKind::UsbPath);
        assert_eq!(keys[0].value, "10C4:EA60@LOC2:if0");
    }

    #[test]
    fn recognizes_renumbered_port_by_serial() {
        let mut known = Vec::new();
        let first = remember(
            &[usb_port(
                "COM5",
                0x0403,
                0x6001,
                Some("A10XYZ12"),
                Some(0),
                "LOC1",
            )],
            &mut known,
            1,
        );
        // Same cable, different socket and different COM number.
        let port = usb_port("COM7", 0x0403, 0x6001, Some("A10XYZ12"), Some(0), "LOC9");
        let m = &match_ports(std::slice::from_ref(&port), &known)[0];
        assert_eq!(m.device_id, Some(first[0]));
        assert_eq!(m.basis, Some(MatchBasis::UsbSerial));
    }

    #[test]
    fn different_serial_in_same_socket_is_a_new_device() {
        let mut known = Vec::new();
        remember(
            &[usb_port(
                "COM5",
                0x0403,
                0x6001,
                Some("AAAA1111"),
                Some(0),
                "LOC1",
            )],
            &mut known,
            1,
        );
        let port = usb_port("COM5", 0x0403, 0x6001, Some("BBBB2222"), Some(0), "LOC1");
        let m = &match_ports(std::slice::from_ref(&port), &known)[0];
        assert_eq!(m.device_id, None, "socket match must be vetoed by serial");
    }

    #[test]
    fn serial_less_device_is_recognized_by_socket() {
        let mut known = Vec::new();
        let ids = remember(
            &[usb_port("COM4", 0x1A86, 0x7523, None, None, "LOC1")],
            &mut known,
            1,
        );
        // Same socket: recognized.
        let same = usb_port("COM4", 0x1A86, 0x7523, None, None, "LOC1");
        let m = &match_ports(std::slice::from_ref(&same), &known)[0];
        assert_eq!(m.device_id, Some(ids[0]));
        assert_eq!(m.basis, Some(MatchBasis::UsbPath));
        // Different socket: cannot be told apart from another CH340, so new.
        let moved = usb_port("COM9", 0x1A86, 0x7523, None, None, "LOC2");
        assert_eq!(match_ports(&[moved], &known)[0].device_id, None);
        // A device with a serial in that socket is not the serial-less one.
        let other = usb_port("COM4", 0x1A86, 0x7523, Some("XYZ98765"), None, "LOC1");
        assert_eq!(match_ports(&[other], &known)[0].device_id, None);
    }

    #[test]
    fn duplicate_serials_in_one_scan_fall_back_to_location() {
        let mut known = Vec::new();
        let ids = remember(
            &[usb_port(
                "COM3",
                0x0403,
                0x6001,
                Some("SAMESERIAL"),
                Some(0),
                "LOC1",
            )],
            &mut known,
            1,
        );
        let ports = [
            usb_port("COM3", 0x0403, 0x6001, Some("SAMESERIAL"), Some(0), "LOC1"),
            usb_port("COM6", 0x0403, 0x6001, Some("SAMESERIAL"), Some(0), "LOC2"),
        ];
        let matches = match_ports(&ports, &known);
        assert!(matches.iter().all(|m| m.ambiguous_serial));
        assert_eq!(matches[0].device_id, Some(ids[0]));
        assert_eq!(matches[0].basis, Some(MatchBasis::UsbPath));
        assert_eq!(matches[1].device_id, None);
        assert!(matches[1].keys.iter().all(|k| k.kind != KeyKind::UsbSerial));
    }

    #[test]
    fn windows_phantom_is_matched_by_instance_id() {
        let mut present = usb_port("COM8", 0x10C4, 0xEA70, Some("01A2B3C4"), Some(1), "LOC1");
        present.instance_identity = Some(InstanceIdentity {
            value: r"USB\VID_10C4&PID_EA70&MI_01\6&2A3B4C5D&0&0001".into(),
            quality: InstanceQuality::Location,
        });
        let mut known = Vec::new();
        let ids = remember(std::slice::from_ref(&present), &mut known, 1);

        // Unplugged: Windows reports a phantom with partial information.
        let phantom = DiscoveredPort {
            port_name: "COM8".into(),
            presence: Presence::Absent,
            transport: Transport::Usb,
            usb: Some(UsbInfo {
                vid: 0x10C4,
                pid: 0xEA70,
                interface_number: Some(1),
                ..Default::default()
            }),
            instance_identity: Some(InstanceIdentity {
                value: r"usb\vid_10c4&pid_ea70&mi_01\6&2a3b4c5d&0&0001".into(),
                quality: InstanceQuality::Location,
            }),
            ..Default::default()
        };
        let m = &match_ports(&[phantom], &known)[0];
        assert_eq!(m.device_id, Some(ids[0]));
        assert_eq!(m.basis, Some(MatchBasis::OsDevice));
    }

    #[test]
    fn each_device_matches_at_most_one_port() {
        let mut known = Vec::new();
        let port_a = usb_port("COM4", 0x1A86, 0x7523, None, None, "LOC1");
        let port_b = usb_port("COM9", 0x1A86, 0x7523, None, None, "LOC2");
        let ids = remember(std::slice::from_ref(&port_a), &mut known, 10);
        // User linked the socket LOC2 to the same device (merge).
        known[0].keys.push(KnownKey {
            key: derive_keys(&port_b)[0].clone(),
            last_seen: 5,
        });
        let matches = match_ports(&[port_b, port_a], &known);
        // The more recently used socket wins; the other port becomes new.
        assert_eq!(matches[1].device_id, Some(ids[0]));
        assert_eq!(matches[0].device_id, None);
    }

    #[test]
    fn bluetooth_ports_match_by_address_and_are_vetoed_by_other_addresses() {
        let bt = |name: &str, addr: &str| DiscoveredPort {
            port_name: name.into(),
            transport: Transport::Bluetooth,
            bluetooth: Some(BluetoothInfo {
                address: Some(addr.into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut known = Vec::new();
        let ids = remember(&[bt("COM20", "001122334455")], &mut known, 1);
        let m = &match_ports(&[bt("COM21", "00:11:22:33:44:55")], &known)[0];
        assert_eq!(m.device_id, Some(ids[0]));
        assert_eq!(m.basis, Some(MatchBasis::Bluetooth));
        let m = &match_ports(&[bt("COM20", "66:77:88:99:AA:BB")], &known)[0];
        assert_eq!(m.device_id, None);
    }

    #[test]
    fn virtual_ports_match_by_provider_and_name() {
        let vport = |name: &str| DiscoveredPort {
            port_name: name.into(),
            transport: Transport::Virtual,
            virtual_port: Some(VirtualInfo {
                provider: "com0com".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut known = Vec::new();
        let ids = remember(&[vport("COM12")], &mut known, 1);
        assert_eq!(derive_keys(&vport("COM12"))[0].value, "com0com:COM12");
        assert_eq!(
            match_ports(&[vport("COM12")], &known)[0].device_id,
            Some(ids[0])
        );
        assert_eq!(match_ports(&[vport("COM13")], &known)[0].device_id, None);
    }

    #[test]
    fn port_name_fallback_only_for_keyless_ports() {
        let keyless = |name: &str| DiscoveredPort {
            port_name: name.into(),
            transport: Transport::Bluetooth,
            ..Default::default()
        };
        let mut known = Vec::new();
        let ids = remember(&[keyless("/dev/cu.HC-05-DevB")], &mut known, 1);
        let m = &match_ports(&[keyless("/dev/cu.HC-05-DevB")], &known)[0];
        assert_eq!(m.device_id, Some(ids[0]));
        assert_eq!(m.basis, Some(MatchBasis::PortName));

        // A USB device that used to live at /dev/ttyUSB0 is never matched by
        // name to whatever gets that name next.
        let mut known = Vec::new();
        remember(
            &[usb_port(
                "/dev/ttyUSB0",
                0x0403,
                0x6001,
                Some("A10XYZ12"),
                Some(0),
                "L1",
            )],
            &mut known,
            1,
        );
        let other = DiscoveredPort {
            port_name: "/dev/ttyUSB0".into(),
            transport: Transport::Usb,
            ..Default::default()
        };
        assert_eq!(match_ports(&[other], &known)[0].device_id, None);
    }

    #[test]
    fn keys_are_sorted_and_deduplicated() {
        let mut port = usb_port("COM1", 1, 2, Some("SERIAL123"), Some(0), "L");
        port.instance_identity = Some(InstanceIdentity {
            value: r"USB\VID_0001&PID_0002\SERIAL123".into(),
            quality: InstanceQuality::DeviceUnique,
        });
        let keys = derive_keys(&port);
        let strengths: Vec<u8> = keys.iter().map(|k| k.strength).collect();
        assert_eq!(strengths, vec![100, 90, 60]);
    }
}
