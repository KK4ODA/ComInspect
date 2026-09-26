//! Normalized, OS-independent description of a serial port as reported by a
//! platform adapter.
//!
//! Every adapter (Windows, Linux, macOS) produces [`DiscoveredPort`] values.
//! Nothing in this module knows how the data was collected; that keeps the
//! identity engine, analysis and UI identical on every operating system.

use serde::{Deserialize, Serialize};

/// How a serial port is attached to the computer.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// USB-UART bridges (FTDI, Silicon Labs, Prolific, WCH …) and USB CDC-ACM devices.
    Usb,
    /// Bluetooth Serial Port Profile (RFCOMM) ports.
    Bluetooth,
    /// Software ports created by a driver or application (com0com, VSPE, tty0tty …).
    Virtual,
    /// Add-in PCI/PCIe serial cards and PCI functions such as Intel AMT SOL.
    Pci,
    /// On-board UARTs (ACPI `PNP0501`, SoC UARTs).
    Builtin,
    #[default]
    Unknown,
}

impl Transport {
    pub const ALL: [Transport; 6] = [
        Transport::Usb,
        Transport::Bluetooth,
        Transport::Virtual,
        Transport::Pci,
        Transport::Builtin,
        Transport::Unknown,
    ];

    /// Stable machine-readable name (used in the database and export files).
    pub fn as_str(self) -> &'static str {
        match self {
            Transport::Usb => "usb",
            Transport::Bluetooth => "bluetooth",
            Transport::Virtual => "virtual",
            Transport::Pci => "pci",
            Transport::Builtin => "builtin",
            Transport::Unknown => "unknown",
        }
    }

    /// Human readable label.
    pub fn label(self) -> &'static str {
        match self {
            Transport::Usb => "USB",
            Transport::Bluetooth => "Bluetooth",
            Transport::Virtual => "Virtual",
            Transport::Pci => "PCI",
            Transport::Builtin => "Built-in",
            Transport::Unknown => "Unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Transport> {
        Transport::ALL
            .into_iter()
            .find(|t| t.as_str().eq_ignore_ascii_case(value))
    }
}

/// Whether the device behind a port is currently attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    /// The device is attached and the OS exposes a usable port for it.
    #[default]
    Present,
    /// The OS still remembers the port (and usually keeps its COM number
    /// reserved) but the device is not attached. These are the Windows
    /// "hidden" or phantom devices.
    Absent,
}

/// USB-specific information about a port.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbInfo {
    pub vid: u16,
    pub pid: u16,
    /// Serial number string reported by the device (`iSerialNumber`).
    /// `None` when the device reports none or when the OS only has a
    /// generated, location-based substitute.
    pub serial_number: Option<String>,
    /// USB interface number (`bInterfaceNumber`, Windows `MI_xx`, FTDI
    /// channel A/B/C/D = 0/1/2/3). Distinguishes the ports of multi-port
    /// devices such as the CP2105 "Enhanced" and "Standard" ports.
    pub interface_number: Option<u8>,
    /// Interface string (`iInterface`) when the OS exposes it.
    pub interface_name: Option<String>,
    /// Manufacturer string (`iManufacturer`) when available.
    pub manufacturer: Option<String>,
    /// Product string (`iProduct`) when available.
    pub product: Option<String>,
    /// Device release number (`bcdDevice`).
    pub revision: Option<u16>,
    /// Stable description of the physical USB socket (topology) in an
    /// OS-specific format. Used as a fallback identity key.
    pub location: Option<String>,
    /// Human readable form of the location.
    pub location_label: Option<String>,
    /// OS identifier of the USB device node (e.g. the Windows instance ID
    /// of the USB device or the Linux sysfs name such as `1-1.4`).
    pub device_node: Option<String>,
}

/// Direction of a Bluetooth serial port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BluetoothDirection {
    /// A local server port that waits for a remote device to connect.
    Incoming,
    /// A port that connects to a specific paired device when opened.
    Outgoing,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothInfo {
    /// Canonical remote address `AA:BB:CC:DD:EE:FF`, `None` for incoming ports.
    pub address: Option<String>,
    pub device_name: Option<String>,
    pub direction: Option<BluetoothDirection>,
    /// Service description, e.g. "Serial Port (SPP)".
    pub service: Option<String>,
    /// OS qualifier distinguishing several serial services of the same
    /// remote device (Windows service instance, Linux RFCOMM channel). `None`
    /// for the default/only service.
    pub service_key: Option<String>,
    pub channel: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualInfo {
    /// Best guess of the software that created the port.
    pub provider: String,
    /// Extra information (e.g. the other end of a null-modem pair).
    pub detail: Option<String>,
    /// `true` when the provider was inferred heuristically rather than
    /// reported by the OS.
    pub heuristic: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProblem {
    /// OS specific problem code (Windows `CM_PROB_*`).
    pub code: u32,
    pub description: String,
}

/// Device history recorded by the operating system itself (Windows).
/// All values are Unix time in milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsTimestamps {
    pub first_install: Option<i64>,
    pub install: Option<i64>,
    pub last_arrival: Option<i64>,
    pub last_removal: Option<i64>,
}

impl OsTimestamps {
    pub fn is_empty(&self) -> bool {
        self.first_install.is_none()
            && self.install.is_none()
            && self.last_arrival.is_none()
            && self.last_removal.is_none()
    }

    /// Best OS estimate of when the device was last attached.
    pub fn last_seen_estimate(&self) -> Option<i64> {
        match (self.last_arrival, self.last_removal) {
            (Some(a), Some(r)) => Some(a.max(r)),
            (a, r) => a.or(r),
        }
    }
}

/// How an OS instance identifier was derived, which determines how much it
/// can be trusted as an identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceQuality {
    /// Derived from something unique to the device (e.g. its serial number).
    DeviceUnique,
    /// Fixed hardware or software (on-board UART, virtual driver instance).
    Stable,
    /// Generated from the topology (USB socket) the device is plugged into.
    Location,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceIdentity {
    pub value: String,
    pub quality: InstanceQuality,
}

/// Unix device-node access information.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessInfo {
    pub readable: bool,
    pub writable: bool,
    pub owner_group: Option<String>,
    /// Permission string such as `crw-rw----`.
    pub mode: Option<String>,
}

/// OS-level identifiers and driver information.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// Windows device instance ID; Linux sysfs path of the tty's device;
    /// macOS IORegistry path.
    pub instance_id: Option<String>,
    pub hardware_ids: Vec<String>,
    pub compatible_ids: Vec<String>,
    pub parent_instance_id: Option<String>,
    pub container_id: Option<String>,
    /// Windows setup class name (`Ports`, `Modem` …) or Linux subsystem.
    pub device_class: Option<String>,
    pub class_guid: Option<String>,
    /// Windows enumerator (`USB`, `FTDIBUS`, `BTHENUM`, `ACPI`, `ROOT` …).
    pub enumerator: Option<String>,
    /// Driver/service name (`silabser`, `FTDIBUS`, `usbser`, `ftdi_sio`,
    /// `cdc_acm`, `AppleUSBFTDI` …).
    pub driver: Option<String>,
    pub driver_provider: Option<String>,
    pub driver_version: Option<String>,
    /// ISO-8601 date.
    pub driver_date: Option<String>,
    pub driver_inf: Option<String>,
    /// Kernel device name, e.g. Windows `\Device\Silabser0` from the
    /// SERIALCOMM map.
    pub kernel_name: Option<String>,
    /// Full device path (Linux sysfs, macOS IORegistry).
    pub device_path: Option<String>,
    pub location_paths: Vec<String>,
    pub location_info: Option<String>,
    pub problem: Option<DeviceProblem>,
    pub access: Option<AccessInfo>,
}

/// A single additional property shown in the inspector's advanced section.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    pub value: String,
}

impl Property {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Property {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// One serial port as seen by the operating system.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPort {
    /// Name applications use to open the port: `COM7`, `/dev/ttyUSB0`,
    /// `/dev/cu.usbserial-A10XYZ12`.
    pub port_name: String,
    /// Alternative paths to the same port (`/dev/serial/by-id/…`,
    /// `/dev/tty.*` dial-in device …).
    pub aliases: Vec<String>,
    pub presence: Presence,
    pub transport: Transport,
    /// OS friendly name (on Windows it includes the `(COMn)` suffix).
    pub friendly_name: Option<String>,
    /// Device description without the port name.
    pub description: Option<String>,
    pub manufacturer: Option<String>,
    pub usb: Option<UsbInfo>,
    pub bluetooth: Option<BluetoothInfo>,
    pub virtual_port: Option<VirtualInfo>,
    pub system: SystemInfo,
    pub os_times: OsTimestamps,
    /// OS instance identifier usable as an identity key, if any.
    pub instance_identity: Option<InstanceIdentity>,
    /// Notes produced by the adapter (e.g. "macOS system port").
    pub notes: Vec<String>,
    /// Additional properties for the inspector.
    pub extra: Vec<Property>,
}

/// Descriptions that tell the user nothing about the device; when one of
/// these is reported we prefer the USB product string.
const GENERIC_DESCRIPTIONS: &[&str] = &[
    "usb serial port",
    "usb serial device",
    "usb-serial",
    "serial port",
    "communications port",
    "usb device",
];

impl DiscoveredPort {
    pub fn is_present(&self) -> bool {
        self.presence == Presence::Present
    }

    /// Short form of the port name for display: `COM7`, `ttyUSB0`,
    /// `cu.usbserial-A10XYZ12`.
    pub fn short_port_name(&self) -> &str {
        short_port_name(&self.port_name)
    }

    /// Best human readable description of the device.
    pub fn device_label(&self) -> String {
        let description = self
            .description
            .as_deref()
            .or(self.friendly_name.as_deref())
            .map(strip_port_suffix)
            .filter(|d| !d.is_empty());
        let product = self
            .usb
            .as_ref()
            .and_then(|u| u.product.as_deref())
            .map(str::trim)
            .filter(|p| !p.is_empty());

        match (description, product) {
            (Some(d), Some(p)) if is_generic_description(&d) => p.to_string(),
            (Some(d), _) => d,
            (None, Some(p)) => p.to_string(),
            (None, None) => {
                if let Some(bt) = &self.bluetooth
                    && let Some(name) = &bt.device_name
                {
                    return name.clone();
                }
                if let Some(v) = &self.virtual_port {
                    return format!("{} virtual port", v.provider);
                }
                self.short_port_name().to_string()
            }
        }
    }

    /// COM port number on Windows-style names (`COM7` → 7).
    pub fn com_number(&self) -> Option<u32> {
        com_number(&self.port_name)
    }
}

/// `true` for vendor-neutral descriptions such as "USB Serial Port".
pub fn is_generic_description(description: &str) -> bool {
    let d = description.trim().to_ascii_lowercase();
    GENERIC_DESCRIPTIONS.iter().any(|g| d == *g)
}

/// Removes a trailing ` (COM7)` from Windows friendly names.
pub fn strip_port_suffix(name: &str) -> String {
    let trimmed = name.trim();
    if let Some(open) = trimmed.rfind(" (")
        && trimmed.ends_with(')')
    {
        let inner = &trimmed[open + 2..trimmed.len() - 1];
        if com_number(inner).is_some() || inner.to_ascii_uppercase().starts_with("LPT") {
            return trimmed[..open].trim_end().to_string();
        }
    }
    trimmed.to_string()
}

/// Extracts the port name from a Windows friendly name such as
/// `Silicon Labs CP210x USB to UART Bridge (COM7)`.
pub fn port_from_friendly_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    let open = trimmed.rfind('(')?;
    let close = trimmed[open..].find(')')? + open;
    let inner = trimmed[open + 1..close].trim();
    com_number(inner).map(|n| format!("COM{n}"))
}

/// Parses `COM7` / `com7` / `\\.\COM7` into 7.
pub fn com_number(name: &str) -> Option<u32> {
    let name = name.trim();
    let name = name.strip_prefix(r"\\.\").unwrap_or(name);
    if name.len() < 4 || !name[..3].eq_ignore_ascii_case("com") {
        return None;
    }
    let digits = &name[3..];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || digits.starts_with('0') {
        return None;
    }
    digits.parse().ok().filter(|n| (1..=4096).contains(n))
}

/// Short form of a port path (`/dev/ttyUSB0` → `ttyUSB0`).
pub fn short_port_name(port: &str) -> &str {
    port.strip_prefix("/dev/").unwrap_or(port)
}

/// Sort key that orders `COM2` before `COM10` and groups by text.
pub fn natural_sort_key(name: &str) -> Vec<NaturalChunk> {
    let mut chunks = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            if !text.is_empty() {
                chunks.push(NaturalChunk::Text(std::mem::take(&mut text).to_lowercase()));
            }
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                chunks.push(NaturalChunk::Number(digits.parse().unwrap_or(u64::MAX)));
                digits.clear();
            }
            text.push(ch);
        }
    }
    if !text.is_empty() {
        chunks.push(NaturalChunk::Text(text.to_lowercase()));
    }
    if !digits.is_empty() {
        chunks.push(NaturalChunk::Number(digits.parse().unwrap_or(u64::MAX)));
    }
    chunks
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NaturalChunk {
    Number(u64),
    Text(String),
}

/// COM numbers reserved in the Windows COM Name Arbiter database.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReservedPorts {
    /// Reserved COM numbers, ascending.
    pub numbers: Vec<u32>,
    /// Number of COM numbers tracked by the database.
    pub database_size: u32,
    /// Where the information came from ("ComDB API" or the read-only
    /// registry fallback).
    pub source: String,
}

impl ReservedPorts {
    /// Decodes the ComDB bit array (bit 0 of byte 0 = COM1).
    pub fn from_bitmap(bytes: &[u8], database_size: u32, source: impl Into<String>) -> Self {
        let limit = (database_size as usize).min(bytes.len() * 8);
        let numbers = (0..limit)
            .filter(|&bit| bytes[bit / 8] & (1 << (bit % 8)) != 0)
            .map(|bit| bit as u32 + 1)
            .collect();
        ReservedPorts {
            numbers,
            database_size,
            source: source.into(),
        }
    }
}

/// What the current platform adapter is able to report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    /// The OS remembers absent devices (Windows hidden devices).
    pub remembers_absent_devices: bool,
    /// Reserved COM numbers are available (Windows ComDB).
    pub reserved_numbers: bool,
    /// The OS records device install/arrival/removal times.
    pub os_timestamps: bool,
    /// Description of the live monitoring mechanism.
    pub monitoring: String,
}

/// Result of one enumeration pass.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    /// `windows`, `linux` or `macos`.
    pub platform: String,
    /// Unix time in milliseconds.
    pub scanned_at: i64,
    pub duration_ms: u64,
    pub ports: Vec<DiscoveredPort>,
    pub reserved: Option<ReservedPorts>,
    /// Non-fatal problems encountered while enumerating.
    pub warnings: Vec<String>,
    pub capabilities: PlatformCapabilities,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_com_numbers() {
        assert_eq!(com_number("COM7"), Some(7));
        assert_eq!(com_number("com12"), Some(12));
        assert_eq!(com_number(r"\\.\COM255"), Some(255));
        assert_eq!(com_number("COM0"), None);
        assert_eq!(com_number("COM07"), None);
        assert_eq!(com_number("COM"), None);
        assert_eq!(com_number("CNCA0"), None);
        assert_eq!(com_number("/dev/ttyUSB0"), None);
    }

    #[test]
    fn strips_port_suffix_from_friendly_names() {
        assert_eq!(
            strip_port_suffix("Silicon Labs CP210x USB to UART Bridge (COM7)"),
            "Silicon Labs CP210x USB to UART Bridge"
        );
        assert_eq!(
            strip_port_suffix("Printer Port (LPT1)"),
            "Printer Port".to_string()
        );
        assert_eq!(
            strip_port_suffix("Intel(R) Active Management Technology - SOL (COM3)"),
            "Intel(R) Active Management Technology - SOL"
        );
        assert_eq!(strip_port_suffix("Gadget (rev 2)"), "Gadget (rev 2)");
    }

    #[test]
    fn extracts_port_from_friendly_name() {
        assert_eq!(
            port_from_friendly_name("USB Serial Port (COM14)").as_deref(),
            Some("COM14")
        );
        assert_eq!(port_from_friendly_name("USB Serial Port"), None);
        assert_eq!(port_from_friendly_name("Something (rev 2)"), None);
    }

    #[test]
    fn natural_sort_orders_numbers_numerically() {
        let mut names = vec!["COM10", "COM2", "COM1", "/dev/ttyUSB10", "/dev/ttyUSB9"];
        names.sort_by_key(|n| natural_sort_key(n));
        assert_eq!(
            names,
            vec!["/dev/ttyUSB9", "/dev/ttyUSB10", "COM1", "COM2", "COM10"]
        );
    }

    #[test]
    fn decodes_comdb_bitmap() {
        // COM1, COM3, COM9 and COM16 reserved.
        let bytes = [0b0000_0101, 0b1000_0001];
        let reserved = ReservedPorts::from_bitmap(&bytes, 16, "test");
        assert_eq!(reserved.numbers, vec![1, 3, 9, 16]);
        // The database size limits how many bits are meaningful.
        let reserved = ReservedPorts::from_bitmap(&bytes, 8, "test");
        assert_eq!(reserved.numbers, vec![1, 3]);
    }

    #[test]
    fn device_label_prefers_product_over_generic_description() {
        let port = DiscoveredPort {
            port_name: "COM5".into(),
            description: Some("USB Serial Port".into()),
            usb: Some(UsbInfo {
                vid: 0x0403,
                pid: 0x6001,
                product: Some("FT232R USB UART".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(port.device_label(), "FT232R USB UART");

        let port = DiscoveredPort {
            port_name: "COM7".into(),
            friendly_name: Some(
                "Silicon Labs Dual CP2105 USB to UART Bridge: Enhanced COM Port (COM7)".into(),
            ),
            ..Default::default()
        };
        assert_eq!(
            port.device_label(),
            "Silicon Labs Dual CP2105 USB to UART Bridge: Enhanced COM Port"
        );

        let port = DiscoveredPort {
            port_name: "/dev/ttyS0".into(),
            ..Default::default()
        };
        assert_eq!(port.device_label(), "ttyS0");
    }

    #[test]
    fn os_timestamps_estimate_latest_event() {
        let t = OsTimestamps {
            last_arrival: Some(100),
            last_removal: Some(200),
            ..Default::default()
        };
        assert_eq!(t.last_seen_estimate(), Some(200));
        let t = OsTimestamps {
            last_arrival: Some(100),
            ..Default::default()
        };
        assert_eq!(t.last_seen_estimate(), Some(100));
        assert!(OsTimestamps::default().is_empty());
    }
}
