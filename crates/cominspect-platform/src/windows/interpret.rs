//! Pure interpretation of raw Windows device data.
//!
//! The FFI layer (`collect.rs`, Windows only) fills [`RawWindowsScan`] with
//! device-node properties exactly as Windows reports them. Everything here is
//! plain string processing so it is compiled and tested on every platform.

use std::collections::HashMap;

use cominspect_core::analysis::windows_problem_description;
use cominspect_core::model::{
    BluetoothDirection, BluetoothInfo, DeviceProblem, DiscoveredPort, InstanceIdentity,
    InstanceQuality, OsTimestamps, PlatformCapabilities, Presence, Property, ReservedPorts,
    ScanResult, SystemInfo, Transport, UsbInfo, VirtualInfo, com_number, port_from_friendly_name,
};

/// Properties of one device node as reported by Configuration Manager.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawDevNode {
    pub instance_id: String,
    pub present: bool,
    /// Problem code when the device is present and has a problem.
    pub problem: Option<u32>,
    pub friendly_name: Option<String>,
    pub device_desc: Option<String>,
    pub bus_reported_desc: Option<String>,
    pub manufacturer: Option<String>,
    pub hardware_ids: Vec<String>,
    pub compatible_ids: Vec<String>,
    pub class: Option<String>,
    pub class_guid: Option<String>,
    pub enumerator: Option<String>,
    pub service: Option<String>,
    pub driver_provider: Option<String>,
    pub driver_version: Option<String>,
    /// Unix ms.
    pub driver_date: Option<i64>,
    pub driver_inf: Option<String>,
    pub location_info: Option<String>,
    pub location_paths: Vec<String>,
    pub container_id: Option<String>,
    pub parent: Option<String>,
    pub install_date: Option<i64>,
    pub first_install_date: Option<i64>,
    pub last_arrival_date: Option<i64>,
    pub last_removal_date: Option<i64>,
    /// `PortName` from the device's hardware key.
    pub port_name: Option<String>,
}

/// A port device node together with its ancestors (nearest first). The
/// ancestor chain is usually empty for non-present devices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawPort {
    pub node: RawDevNode,
    pub ancestors: Vec<RawDevNode>,
}

/// One value of `HKLM\HARDWARE\DEVICEMAP\SERIALCOMM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawSerialComm {
    /// Value name, e.g. `\Device\Silabser0`.
    pub kernel_name: String,
    /// Value data, e.g. `COM7`.
    pub port_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawComDb {
    pub bitmap: Vec<u8>,
    pub database_size: u32,
    pub source: String,
}

#[derive(Clone, Debug, Default)]
pub struct RawWindowsScan {
    pub ports: Vec<RawPort>,
    pub serialcomm: Vec<RawSerialComm>,
    /// Bluetooth address (12 upper-case hex digits) → device name.
    pub bluetooth_names: HashMap<String, String>,
    pub comdb: Option<RawComDb>,
    pub warnings: Vec<String>,
}

pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        remembers_absent_devices: true,
        reserved_numbers: true,
        os_timestamps: true,
        monitoring: "Plug and Play notifications + serial device map".into(),
    }
}

// --- ID parsing ---------------------------------------------------------------

/// Enumerator (first path segment) of an instance ID, upper-case.
pub fn enumerator_of(instance_id: &str) -> String {
    instance_id
        .split('\\')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase()
}

/// Last segment (unique ID) of an instance ID.
pub fn unique_part(instance_id: &str) -> &str {
    instance_id.rsplit('\\').next().unwrap_or_default()
}

/// Middle segment (device ID without the enumerator) of an instance ID.
fn device_part(instance_id: &str) -> &str {
    let mut parts = instance_id.splitn(3, '\\');
    parts.next();
    parts.next().unwrap_or_default()
}

/// Windows generates location-based unique IDs such as `5&2a3b4c5d&0&3`;
/// device-provided ones (serial numbers) never contain `&`.
pub fn is_generated_unique_id(part: &str) -> bool {
    part.contains('&')
}

fn hex_after(haystack: &str, tag: &str, digits: usize) -> Option<u16> {
    let upper = haystack.to_ascii_uppercase();
    let start = upper.find(tag)? + tag.len();
    let hex = upper.get(start..start + digits)?;
    u16::from_str_radix(hex, 16).ok()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbIds {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub interface: Option<u8>,
    pub revision: Option<u16>,
}

/// Parses `VID_`, `PID_`, `MI_` and `REV_` from a hardware or instance ID.
pub fn parse_usb_ids(id: &str) -> UsbIds {
    UsbIds {
        vid: hex_after(id, "VID_", 4),
        pid: hex_after(id, "PID_", 4),
        interface: hex_after(id, "MI_", 2).map(|v| v as u8),
        revision: hex_after(id, "REV_", 4),
    }
}

/// `true` for the instance ID of a USB *device* (not an interface).
pub fn is_usb_device_id(instance_id: &str) -> bool {
    enumerator_of(instance_id) == "USB"
        && device_part(instance_id)
            .to_ascii_uppercase()
            .contains("VID_")
        && !device_part(instance_id)
            .to_ascii_uppercase()
            .contains("&MI_")
}

/// Serial number embedded in a USB device instance ID, if device-provided.
pub fn usb_serial_from_instance(instance_id: &str) -> Option<String> {
    let part = unique_part(instance_id);
    (!part.is_empty() && !is_generated_unique_id(part)).then(|| part.to_string())
}

/// FTDI VCP ports: `FTDIBUS\VID_0403+PID_6001+A10XYZ12A\0000` → serial
/// `A10XYZ12`, channel 0 (A).
pub fn ftdi_serial_and_channel(instance_id: &str) -> Option<(Option<String>, u8)> {
    if enumerator_of(instance_id) != "FTDIBUS" {
        return None;
    }
    let tail = device_part(instance_id).split('+').nth(2)?;
    if tail.is_empty() {
        return None;
    }
    let last = tail.chars().last()?.to_ascii_uppercase();
    let (serial, channel) = match last {
        'A'..='D' if tail.len() > 1 => (&tail[..tail.len() - 1], last as u8 - b'A'),
        _ => (tail, 0),
    };
    let serial =
        (!is_generated_unique_id(serial) && !serial.is_empty()).then(|| serial.to_string());
    Some((serial, channel))
}

/// Bluetooth address of a BTHENUM port or device instance ID
/// (12 upper-case hex digits, possibly all zero for incoming ports), plus the
/// service-instance suffix.
pub fn bluetooth_address(instance_id: &str) -> Option<(String, Option<String>)> {
    if enumerator_of(instance_id) != "BTHENUM" {
        return None;
    }
    let device = device_part(instance_id);
    if let Some(rest) = device.to_ascii_uppercase().strip_prefix("DEV_") {
        let hex: String = rest.chars().take(12).collect();
        return (hex.len() == 12 && hex.chars().all(|c| c.is_ascii_hexdigit()))
            .then_some((hex, None));
    }
    let last = unique_part(instance_id).rsplit('&').next()?;
    let mut pieces = last.splitn(2, '_');
    let hex = pieces.next()?.to_ascii_uppercase();
    if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let suffix = pieces.next().map(|s| s.to_ascii_uppercase());
    Some((hex, suffix))
}

fn format_bt_address(hex: &str) -> String {
    (0..6)
        .map(|i| &hex[i * 2..i * 2 + 2])
        .collect::<Vec<_>>()
        .join(":")
}

fn bluetooth_service_name(instance_id: &str) -> Option<String> {
    let device = device_part(instance_id).to_ascii_uppercase();
    let guid = device.strip_prefix('{')?.split('}').next()?.to_string();
    Some(match guid.split('-').next()? {
        "00001101" => "Serial Port (SPP)".to_string(),
        "00001103" => "Dial-up Networking (DUN)".to_string(),
        _ => format!("Service {{{guid}}}"),
    })
}

// --- classification -------------------------------------------------------------

fn any_id(port: &RawPort, pred: impl Fn(&str) -> bool) -> bool {
    std::iter::once(&port.node)
        .chain(port.ancestors.iter())
        .any(|n| pred(&n.instance_id.to_ascii_uppercase()))
}

/// Classifies how the port is attached.
pub fn classify(port: &RawPort, kernel_name: Option<&str>) -> Transport {
    let node = &port.node;
    let enumerator = node
        .enumerator
        .as_deref()
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| enumerator_of(&node.instance_id));
    let kernel = kernel_name.unwrap_or_default().to_ascii_lowercase();

    if enumerator == "BTHENUM"
        || any_id(port, |id| id.starts_with("BTHENUM\\"))
        || kernel.contains("bthmodem")
    {
        return Transport::Bluetooth;
    }
    if matches!(enumerator.as_str(), "USB" | "FTDIBUS" | "USBSER")
        || any_id(port, |id| id.starts_with("USB\\"))
    {
        return Transport::Usb;
    }
    if enumerator == "ACPI" || enumerator == "ACPI_HAL" {
        return Transport::Builtin;
    }
    if enumerator == "PCI" || any_id(port, |id| id.starts_with("PCI\\")) {
        return Transport::Pci;
    }
    if matches!(enumerator.as_str(), "ROOT" | "SWD" | "COM0COM")
        || port
            .ancestors
            .first()
            .is_some_and(|p| enumerator_of(&p.instance_id) == "ROOT")
    {
        return Transport::Virtual;
    }
    if node
        .hardware_ids
        .iter()
        .any(|h| h.to_ascii_uppercase().starts_with("USB\\"))
    {
        return Transport::Usb;
    }
    if node
        .hardware_ids
        .iter()
        .any(|h| h.to_ascii_uppercase().starts_with("PCI\\"))
    {
        return Transport::Pci;
    }
    Transport::Unknown
}

/// Best guess of the software behind a virtual port, from the enumerator,
/// kernel device name, driver provider, service and description.
pub fn virtual_provider(port: &RawPort, kernel_name: Option<&str>) -> (String, bool) {
    let node = &port.node;
    let enumerator = enumerator_of(&node.instance_id);
    if enumerator == "COM0COM" {
        return ("com0com".into(), false);
    }
    let haystack = [
        kernel_name.unwrap_or_default(),
        node.driver_provider.as_deref().unwrap_or_default(),
        node.service.as_deref().unwrap_or_default(),
        node.device_desc.as_deref().unwrap_or_default(),
        node.friendly_name.as_deref().unwrap_or_default(),
        node.manufacturer.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    let known: &[(&[&str], &str)] = &[
        (&["com0com"], "com0com"),
        (&["vspe", "eterlogic"], "VSPE"),
        (
            &["eltima", "electronic team", "evserial", "vspd"],
            "Eltima Virtual Serial Port",
        ),
        (&["hwvsp", "hw vsp", "hw group"], "HW VSP"),
        (&["fabulatech"], "FabulaTech Virtual Serial Port"),
        (
            &["flexradio", "flexvsp", "smartsdr"],
            "FlexRadio SmartSDR CAT",
        ),
        (&["n8vb", "vcom"], "N8VB vCOM"),
    ];
    for (needles, name) in known {
        if needles.iter().any(|n| haystack.contains(n)) {
            return ((*name).to_string(), true);
        }
    }
    let fallback = node
        .driver_provider
        .clone()
        .filter(|p| !p.eq_ignore_ascii_case("microsoft"))
        .unwrap_or_else(|| "Unknown virtual-port driver".into());
    (fallback, true)
}

/// Classification of ports that appear only in the SERIALCOMM map (no
/// Plug and Play device node).
pub fn classify_kernel_name(kernel_name: &str) -> Transport {
    let k = kernel_name.to_ascii_lowercase();
    if k.contains("bthmodem") {
        Transport::Bluetooth
    } else if [
        "\\device\\vcp",
        "\\device\\silabser",
        "\\device\\prolificserial",
        "\\device\\usbser",
        "\\device\\ch341ser",
    ]
    .iter()
    .any(|p| k.starts_with(p))
    {
        Transport::Usb
    } else if k.starts_with("\\device\\serial") {
        Transport::Builtin
    } else {
        Transport::Virtual
    }
}

// --- interpretation --------------------------------------------------------------

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// Finds the USB device node of a port: the node itself for non-composite
/// devices, otherwise the nearest ancestor.
fn usb_device_node(port: &RawPort) -> Option<&RawDevNode> {
    std::iter::once(&port.node)
        .chain(port.ancestors.iter())
        .find(|n| is_usb_device_id(&n.instance_id))
}

fn build_usb(port: &RawPort) -> Option<UsbInfo> {
    let node = &port.node;
    let ftdi = ftdi_serial_and_channel(&node.instance_id);
    let usb_node = usb_device_node(port);

    // VID/PID: from the port's own ID, else its hardware IDs, else the USB node.
    let mut ids = parse_usb_ids(&node.instance_id);
    for hw in &node.hardware_ids {
        let h = parse_usb_ids(hw);
        ids.vid = ids.vid.or(h.vid);
        ids.pid = ids.pid.or(h.pid);
        ids.revision = ids.revision.or(h.revision);
        ids.interface = ids.interface.or(h.interface);
    }
    if let Some(u) = usb_node {
        let h = parse_usb_ids(&u.instance_id);
        ids.vid = ids.vid.or(h.vid);
        ids.pid = ids.pid.or(h.pid);
        for hw in &u.hardware_ids {
            ids.revision = ids.revision.or(parse_usb_ids(hw).revision);
        }
    }
    // Interface number from the nearest interface node.
    if ids.interface.is_none() {
        ids.interface = port
            .ancestors
            .iter()
            .find_map(|a| parse_usb_ids(&a.instance_id).interface);
    }
    let (vid, pid) = (ids.vid?, ids.pid?);

    let (serial, interface) = match ftdi {
        Some((serial, channel)) => (
            serial.or_else(|| usb_node.and_then(|u| usb_serial_from_instance(&u.instance_id))),
            Some(channel),
        ),
        None => (
            usb_node.and_then(|u| usb_serial_from_instance(&u.instance_id)),
            Some(ids.interface.unwrap_or(0)),
        ),
    };

    let location_node = usb_node.unwrap_or(node);
    Some(UsbInfo {
        vid,
        pid,
        serial_number: serial,
        interface_number: interface,
        interface_name: None,
        manufacturer: None,
        product: usb_node
            .and_then(|u| clean(&u.bus_reported_desc))
            .or_else(|| clean(&node.bus_reported_desc)),
        revision: ids.revision,
        location: location_node.location_paths.first().cloned(),
        location_label: clean(&location_node.location_info),
        device_node: usb_node.map(|u| u.instance_id.clone()),
    })
}

fn instance_quality(
    port: &RawPort,
    transport: Transport,
    usb: Option<&UsbInfo>,
) -> InstanceQuality {
    let id = &port.node.instance_id;
    match transport {
        Transport::Usb => {
            if usb.is_some_and(|u| u.serial_number.is_some()) {
                InstanceQuality::DeviceUnique
            } else if is_generated_unique_id(unique_part(id)) {
                InstanceQuality::Location
            } else {
                InstanceQuality::DeviceUnique
            }
        }
        Transport::Bluetooth => match bluetooth_address(id) {
            Some((hex, _)) if hex.chars().any(|c| c != '0') => InstanceQuality::DeviceUnique,
            _ => InstanceQuality::Stable,
        },
        Transport::Pci | Transport::Builtin | Transport::Virtual => InstanceQuality::Stable,
        Transport::Unknown => {
            if is_generated_unique_id(unique_part(id)) {
                InstanceQuality::Location
            } else {
                InstanceQuality::Stable
            }
        }
    }
}

fn iso_date(ms: i64) -> String {
    let text = cominspect_core::time::to_rfc3339(ms);
    text.get(..10).unwrap_or(&text).to_string()
}

/// Turns one raw port into a [`DiscoveredPort`]. Returns `None` for devices
/// without a COM-style port name (e.g. printer ports in the Ports class).
pub fn interpret_port(
    port: &RawPort,
    serialcomm: &[RawSerialComm],
    bluetooth_names: &HashMap<String, String>,
) -> Option<DiscoveredPort> {
    let node = &port.node;
    let port_name = node
        .port_name
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .or_else(|| {
            node.friendly_name
                .as_deref()
                .and_then(port_from_friendly_name)
        })?;
    let kernel_name = serialcomm
        .iter()
        .find(|s| s.port_name.eq_ignore_ascii_case(&port_name))
        .map(|s| s.kernel_name.clone());
    if com_number(&port_name).is_none() && kernel_name.is_none() {
        return None;
    }

    let transport = classify(port, kernel_name.as_deref());
    let usb = match transport {
        Transport::Usb => build_usb(port),
        _ => None,
    };

    let bluetooth = if transport == Transport::Bluetooth {
        let parsed = bluetooth_address(&node.instance_id);
        let (address, suffix) = match &parsed {
            Some((hex, suffix)) if hex.chars().any(|c| c != '0') => {
                (Some(format_bt_address(hex)), suffix.clone())
            }
            _ => (None, None),
        };
        let direction = parsed.as_ref().map(|(hex, _)| {
            if hex.chars().all(|c| c == '0') {
                BluetoothDirection::Incoming
            } else {
                BluetoothDirection::Outgoing
            }
        });
        Some(BluetoothInfo {
            device_name: parsed
                .as_ref()
                .and_then(|(hex, _)| bluetooth_names.get(hex).cloned()),
            address,
            direction,
            service: bluetooth_service_name(&node.instance_id),
            service_key: suffix.filter(|s| s != "C00000000" && s != "00000000"),
            channel: None,
        })
    } else {
        None
    };

    let virtual_port = if transport == Transport::Virtual {
        let (provider, heuristic) = virtual_provider(port, kernel_name.as_deref());
        Some(VirtualInfo {
            provider,
            detail: None,
            heuristic,
        })
    } else {
        None
    };

    let present = node.present;
    let problem = node.problem.filter(|_| present).map(|code| DeviceProblem {
        code,
        description: windows_problem_description(code),
    });

    let instance_identity = Some(InstanceIdentity {
        value: node.instance_id.clone(),
        quality: instance_quality(port, transport, usb.as_ref()),
    });

    let mut extra = Vec::new();
    if let Some(u) = usb_device_node(port)
        && u.instance_id != node.instance_id
    {
        extra.push(Property::new("USB device instance", u.instance_id.clone()));
    }
    if let Some(bus) = clean(&node.bus_reported_desc) {
        extra.push(Property::new("Bus-reported description", bus));
    }
    for ancestor in port.ancestors.iter().take(4) {
        extra.push(Property::new("Parent device", ancestor.instance_id.clone()));
    }

    let system = SystemInfo {
        instance_id: Some(node.instance_id.clone()),
        hardware_ids: node.hardware_ids.clone(),
        compatible_ids: node.compatible_ids.clone(),
        parent_instance_id: node
            .parent
            .clone()
            .or_else(|| port.ancestors.first().map(|a| a.instance_id.clone())),
        container_id: clean(&node.container_id),
        device_class: clean(&node.class),
        class_guid: clean(&node.class_guid),
        enumerator: node
            .enumerator
            .clone()
            .or_else(|| Some(enumerator_of(&node.instance_id))),
        driver: clean(&node.service),
        driver_provider: clean(&node.driver_provider),
        driver_version: clean(&node.driver_version),
        driver_date: node.driver_date.map(iso_date),
        driver_inf: clean(&node.driver_inf),
        kernel_name,
        device_path: None,
        location_paths: node.location_paths.clone(),
        location_info: clean(&node.location_info),
        problem,
        access: None,
    };

    Some(DiscoveredPort {
        port_name,
        aliases: Vec::new(),
        presence: if present {
            Presence::Present
        } else {
            Presence::Absent
        },
        transport,
        friendly_name: clean(&node.friendly_name),
        description: clean(&node.device_desc),
        manufacturer: clean(&node.manufacturer),
        usb,
        bluetooth,
        virtual_port,
        system,
        os_times: OsTimestamps {
            first_install: node.first_install_date,
            install: node.install_date,
            last_arrival: node.last_arrival_date,
            last_removal: node.last_removal_date,
        },
        instance_identity,
        notes: Vec::new(),
        extra,
    })
}

/// A SERIALCOMM entry without a Plug and Play device node.
fn interpret_serialcomm_only(entry: &RawSerialComm) -> DiscoveredPort {
    let transport = classify_kernel_name(&entry.kernel_name);
    let virtual_port = (transport == Transport::Virtual).then(|| {
        let raw = RawPort::default();
        let (provider, heuristic) = virtual_provider(&raw, Some(&entry.kernel_name));
        VirtualInfo {
            provider,
            detail: None,
            heuristic,
        }
    });
    DiscoveredPort {
        port_name: entry.port_name.trim().to_string(),
        presence: Presence::Present,
        transport,
        description: Some(match transport {
            Transport::Virtual => "Virtual serial port (no Plug and Play device)".to_string(),
            _ => "Serial port (no Plug and Play device)".to_string(),
        }),
        virtual_port,
        system: SystemInfo {
            kernel_name: Some(entry.kernel_name.clone()),
            ..Default::default()
        },
        notes: vec![
            "This port is listed in the Windows serial device map but has no Plug and Play \
             device, which is typical of virtual-port software."
                .into(),
        ],
        ..Default::default()
    }
}

/// Links the two ends of com0com pairs (`CNCA0` ↔ `CNCB0`).
fn link_com0com_pairs(ports: &mut [DiscoveredPort]) {
    let names: HashMap<String, String> = ports
        .iter()
        .filter_map(|p| {
            let id = p.system.instance_id.as_deref()?;
            (enumerator_of(id) == "COM0COM")
                .then(|| (unique_part(id).to_ascii_uppercase(), p.port_name.clone()))
        })
        .collect();
    for port in ports.iter_mut() {
        let Some(id) = port.system.instance_id.clone() else {
            continue;
        };
        if enumerator_of(&id) != "COM0COM" {
            continue;
        }
        let me = unique_part(&id).to_ascii_uppercase();
        let partner = if let Some(n) = me.strip_prefix("CNCA") {
            format!("CNCB{n}")
        } else if let Some(n) = me.strip_prefix("CNCB") {
            format!("CNCA{n}")
        } else {
            continue;
        };
        if let Some(v) = port.virtual_port.as_mut() {
            v.detail = Some(match names.get(&partner) {
                Some(name) => format!("Paired with {name} ({partner})"),
                None => format!("Paired with {partner}"),
            });
        }
    }
}

/// Interprets a complete raw scan.
pub fn interpret(raw: RawWindowsScan, scanned_at: i64, duration_ms: u64) -> ScanResult {
    let mut ports: Vec<DiscoveredPort> = raw
        .ports
        .iter()
        .filter_map(|p| interpret_port(p, &raw.serialcomm, &raw.bluetooth_names))
        .collect();

    // Active ports from the serial device map that no device node claims.
    for entry in &raw.serialcomm {
        let claimed = ports
            .iter()
            .any(|p| p.is_present() && p.port_name.eq_ignore_ascii_case(entry.port_name.trim()));
        if !claimed && !entry.port_name.trim().is_empty() {
            ports.push(interpret_serialcomm_only(entry));
        }
    }
    link_com0com_pairs(&mut ports);
    ports.sort_by(|a, b| {
        cominspect_core::model::natural_sort_key(&a.port_name)
            .cmp(&cominspect_core::model::natural_sort_key(&b.port_name))
            .then(b.is_present().cmp(&a.is_present()))
    });

    ScanResult {
        platform: "windows".into(),
        scanned_at,
        duration_ms,
        ports,
        reserved: raw
            .comdb
            .map(|c| ReservedPorts::from_bitmap(&c.bitmap, c.database_size, c.source)),
        warnings: raw.warnings,
        capabilities: capabilities(),
    }
}

#[cfg(test)]
mod tests;
