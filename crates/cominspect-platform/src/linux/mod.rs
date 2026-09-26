//! Linux adapter: sysfs enumeration and netlink uevent monitoring.
//!
//! Linux does not remember absent devices, so every port reported here is
//! present. History of absent devices comes from the ComInspect database.

mod netlink;

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cominspect_core::model::{
    AccessInfo, BluetoothInfo, DiscoveredPort, InstanceIdentity, InstanceQuality,
    PlatformCapabilities, Presence, Property, ScanResult, Transport, UsbInfo, VirtualInfo,
};
use cominspect_core::time::now_ms;

pub(crate) use netlink::start_sources;

pub fn discover() -> ScanResult {
    discover_at(Path::new("/sys"), Path::new("/dev"))
}

pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        remembers_absent_devices: false,
        reserved_numbers: false,
        os_timestamps: false,
        monitoring: "Kernel uevents (netlink)".into(),
    }
}

/// Enumerates serial ports using the given sysfs and /dev roots (the roots
/// are parameters so that tests can use fixture trees).
pub fn discover_at(sys: &Path, dev: &Path) -> ScanResult {
    let started = Instant::now();
    let mut warnings = Vec::new();
    let mut aliases: HashMap<String, Vec<String>> = HashMap::new();
    for sub in ["serial/by-id", "serial/by-path"] {
        collect_aliases(&dev.join(sub), &mut aliases);
    }

    let class = sys.join("class/tty");
    let mut ports = Vec::new();
    match fs::read_dir(&class) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some(port) = inspect_tty(sys, dev, &name, &aliases) {
                    ports.push(port);
                }
            }
        }
        Err(e) => warnings.push(format!("cannot read {}: {e}", class.display())),
    }
    ports.sort_by_key(|p| cominspect_core::model::natural_sort_key(&p.port_name));

    ScanResult {
        platform: "linux".into(),
        scanned_at: now_ms(),
        duration_ms: started.elapsed().as_millis() as u64,
        ports,
        reserved: None,
        warnings,
        capabilities: capabilities(),
    }
}

fn read_attr(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_hex_u16(dir: &Path, name: &str) -> Option<u16> {
    read_attr(dir, name).and_then(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok())
}

/// Basename of the target of a symlink such as `subsystem` or `driver`.
fn link_name(path: &Path) -> Option<String> {
    fs::read_link(path)
        .ok()
        .and_then(|t| t.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// Maps `/dev/serial/by-*/<link>` symlinks to tty names.
fn collect_aliases(dir: &Path, out: &mut HashMap<String, Vec<String>>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(target) = fs::read_link(&path) else {
            continue;
        };
        if let Some(tty) = target.file_name() {
            out.entry(tty.to_string_lossy().into_owned())
                .or_default()
                .push(path.to_string_lossy().into_owned());
        }
    }
    for list in out.values_mut() {
        list.sort();
        list.dedup();
    }
}

/// `true` for sysfs directories that are USB interfaces.
fn is_usb_interface(dir: &Path) -> bool {
    dir.join("bInterfaceNumber").exists()
}

/// `true` for sysfs directories that are USB devices.
fn is_usb_device(dir: &Path) -> bool {
    dir.join("idVendor").exists() && dir.join("idProduct").exists()
}

fn uart_type_name(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "8250",
        2 => "16450",
        3 => "16550",
        4 => "16550A",
        5 => "Cirrus",
        6 => "16650",
        7 => "16650V2",
        8 => "16750",
        9 => "Startech",
        10 => "16C950/954",
        11 => "16654",
        12 => "16850",
        13 => "RSA",
        _ => return None,
    })
}

fn pci_vendor_name(vendor: u16) -> Option<&'static str> {
    Some(match vendor {
        0x8086 => "Intel",
        0x1415 => "Oxford Semiconductor",
        0x13A8 => "Exar",
        0x9710 => "MosChip",
        0x125B => "ASIX",
        0x1C00 => "WCH",
        0x1407 => "Lava",
        0x131F => "SIIG",
        0x1022 => "AMD",
        _ => return None,
    })
}

/// Legacy DOS name of the standard PC I/O ports.
fn legacy_com_name(io_port: &str) -> Option<&'static str> {
    match io_port.to_ascii_uppercase().trim_start_matches("0X") {
        "3F8" => Some("COM1"),
        "2F8" => Some("COM2"),
        "3E8" => Some("COM3"),
        "2E8" => Some("COM4"),
        _ => None,
    }
}

fn access_info(dev_path: &Path) -> Option<AccessInfo> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta = fs::metadata(dev_path).ok()?;
    let c_path = CString::new(dev_path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: c_path is a valid NUL-terminated string.
    let readable = unsafe { libc::access(c_path.as_ptr(), libc::R_OK) } == 0;
    // SAFETY: as above.
    let writable = unsafe { libc::access(c_path.as_ptr(), libc::W_OK) } == 0;
    Some(AccessInfo {
        readable,
        writable,
        owner_group: group_name(meta.gid()),
        mode: Some(mode_string(meta.permissions().mode(), meta.file_type())),
    })
}

fn group_name(gid: u32) -> Option<String> {
    let mut buf = vec![0u8; 4096];
    // SAFETY: zeroed `group` is a valid out-parameter for getgrgid_r.
    let mut group: libc::group = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::group = std::ptr::null_mut();
    // SAFETY: all pointers are valid for the duration of the call and the
    // buffer length matches the allocation.
    let rc = unsafe {
        libc::getgrgid_r(
            gid,
            &mut group,
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() || group.gr_name.is_null() {
        return None;
    }
    // SAFETY: getgrgid_r succeeded, so gr_name points to a NUL-terminated
    // string inside `buf`.
    let name = unsafe { std::ffi::CStr::from_ptr(group.gr_name) };
    Some(name.to_string_lossy().into_owned())
}

fn mode_string(mode: u32, file_type: fs::FileType) -> String {
    use std::os::unix::fs::FileTypeExt;
    let kind = if file_type.is_char_device() {
        'c'
    } else if file_type.is_symlink() {
        'l'
    } else if file_type.is_dir() {
        'd'
    } else {
        '-'
    };
    let mut s = String::with_capacity(10);
    s.push(kind);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

/// Path relative to the sysfs root, formatted like the real `/sys/...` path.
fn sys_display(sys: &Path, path: &Path) -> String {
    match path.strip_prefix(sys) {
        Ok(rel) => format!("/sys/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

fn inspect_tty(
    sys: &Path,
    dev: &Path,
    name: &str,
    aliases: &HashMap<String, Vec<String>>,
) -> Option<DiscoveredPort> {
    let class_entry = sys.join("class/tty").join(name);
    let tty_dir = fs::canonicalize(&class_entry).unwrap_or_else(|_| class_entry.clone());
    let dev_path = dev.join(name);

    let mut port = DiscoveredPort {
        port_name: format!("/dev/{name}"),
        presence: Presence::Present,
        ..Default::default()
    };
    port.aliases = aliases.get(name).cloned().unwrap_or_default();
    port.system.kernel_name = read_attr(&tty_dir, "dev");
    port.system.device_path = Some(sys_display(sys, &tty_dir));
    port.system.access = access_info(&dev_path);
    port.system.location_paths = port
        .aliases
        .iter()
        .filter(|a| a.contains("/by-path/"))
        .cloned()
        .collect();

    let device_link = class_entry.join("device");
    if !device_link.exists() {
        return inspect_virtual_tty(name, &tty_dir, port);
    }
    let device_dir = fs::canonicalize(&device_link).ok()?;

    if name.starts_with("ttyS") {
        // 8250 placeholders without hardware report type 0 (PORT_UNKNOWN).
        match read_attr(&tty_dir, "type").and_then(|t| t.parse::<u32>().ok()) {
            Some(0) | None => return None,
            Some(_) => {}
        }
    }

    // Walk up the tree to find the USB interface/device and the first
    // "hardware" ancestor (not a serial-core helper device).
    let mut usb_interface: Option<PathBuf> = None;
    let mut usb_device: Option<PathBuf> = None;
    let mut hardware: Option<(PathBuf, String)> = None;
    let mut cursor = Some(device_dir.as_path());
    while let Some(dir) = cursor {
        if !dir.starts_with(sys) || dir == sys.join("devices") {
            break;
        }
        if usb_interface.is_none() && is_usb_interface(dir) {
            usb_interface = Some(dir.to_path_buf());
        }
        if is_usb_device(dir) {
            usb_device = Some(dir.to_path_buf());
            break;
        }
        if hardware.is_none()
            && let Some(sub) = link_name(&dir.join("subsystem"))
            && !matches!(sub.as_str(), "serial-base" | "tty" | "usb-serial")
        {
            hardware = Some((dir.to_path_buf(), sub));
        }
        cursor = dir.parent();
    }

    let device_subsystem = link_name(&device_dir.join("subsystem"));
    port.system.device_class.clone_from(&device_subsystem);
    port.system.instance_id = Some(sys_display(sys, &device_dir));
    let modalias = read_attr(&device_dir, "modalias").or_else(|| {
        usb_interface
            .as_deref()
            .and_then(|i| read_attr(i, "modalias"))
    });
    port.system.hardware_ids.extend(modalias);

    if let Some(usb_dir) = usb_device {
        fill_usb(
            sys,
            name,
            &device_dir,
            usb_interface.as_deref(),
            &usb_dir,
            &mut port,
        );
        return Some(port);
    }

    // Not USB: on-board, PCI or something else.
    let (hw_dir, hw_subsystem) = hardware.unwrap_or_else(|| {
        (
            device_dir.clone(),
            device_subsystem.clone().unwrap_or_default(),
        )
    });
    port.system.driver =
        link_name(&hw_dir.join("driver")).or_else(|| link_name(&device_dir.join("driver")));
    let uart = read_attr(&tty_dir, "type")
        .and_then(|t| t.parse::<u32>().ok())
        .and_then(uart_type_name);
    if let Some(io) = read_attr(&tty_dir, "port").filter(|p| p != "0x0") {
        if let Some(legacy) = legacy_com_name(&io) {
            port.extra.push(Property::new("Legacy PC name", legacy));
        }
        port.extra.push(Property::new("I/O port", io));
    }
    if let Some(irq) = read_attr(&tty_dir, "irq") {
        port.extra.push(Property::new("IRQ", irq));
    }
    if let Some(uart) = uart {
        port.extra.push(Property::new("UART", uart));
    }
    match hw_subsystem.as_str() {
        "pci" => {
            port.transport = Transport::Pci;
            let vendor = read_hex_u16(&hw_dir, "vendor");
            let device = read_hex_u16(&hw_dir, "device");
            port.manufacturer = vendor.and_then(pci_vendor_name).map(str::to_string);
            port.description = Some(match (vendor, device) {
                (Some(v), Some(d)) => format!("PCI serial port ({v:04X}:{d:04X})"),
                _ => "PCI serial port".to_string(),
            });
            if let (Some(v), Some(d)) = (vendor, device) {
                port.system
                    .hardware_ids
                    .push(format!("PCI\\VEN_{v:04X}&DEV_{d:04X}"));
            }
        }
        "pnp" | "platform" | "amba" | "acpi" | "serial-base" => {
            port.transport = Transport::Builtin;
            port.description = Some(match uart {
                Some(u) => format!("Built-in serial port ({u} UART)"),
                None => "Built-in serial port".to_string(),
            });
            if let Some(id) = read_attr(&hw_dir, "id") {
                port.system.hardware_ids.insert(0, id);
            }
        }
        _ => {
            port.transport = Transport::Unknown;
            port.description = Some("Serial port".to_string());
        }
    }
    port.system.enumerator = Some(hw_subsystem.clone());
    port.instance_identity = Some(InstanceIdentity {
        value: format!("linux:{}#{name}", sys_display(sys, &hw_dir)),
        quality: InstanceQuality::Stable,
    });
    Some(port)
}

fn fill_usb(
    sys: &Path,
    name: &str,
    device_dir: &Path,
    interface_dir: Option<&Path>,
    usb_dir: &Path,
    port: &mut DiscoveredPort,
) {
    let vid = read_hex_u16(usb_dir, "idVendor").unwrap_or_default();
    let pid = read_hex_u16(usb_dir, "idProduct").unwrap_or_default();
    let interface_number = interface_dir
        .and_then(|i| read_attr(i, "bInterfaceNumber"))
        .and_then(|n| u8::from_str_radix(&n, 16).ok());
    let devpath = read_attr(usb_dir, "devpath");
    let busnum = read_attr(usb_dir, "busnum");
    let node = usb_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());

    // Controller-relative location, as used by udev's by-path links:
    // pci-0000:00:14.0-usb-0:1.4
    let root_hub = usb_dir.ancestors().find(|a| {
        a.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("usb"))
            && !is_usb_interface(a)
    });
    let location = match (root_hub.and_then(Path::parent), devpath.as_deref()) {
        (Some(controller), Some(devpath)) => {
            let id = controller
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let prefix = link_name(&controller.join("subsystem")).unwrap_or_else(|| "usb".into());
            Some(format!("{prefix}-{id}-usb-0:{devpath}"))
        }
        _ => node.clone(),
    };

    let usb = UsbInfo {
        vid,
        pid,
        serial_number: read_attr(usb_dir, "serial"),
        interface_number,
        interface_name: interface_dir.and_then(|i| read_attr(i, "interface")),
        manufacturer: read_attr(usb_dir, "manufacturer"),
        product: read_attr(usb_dir, "product"),
        revision: read_hex_u16(usb_dir, "bcdDevice"),
        location,
        location_label: match (busnum, devpath) {
            (Some(b), Some(d)) => Some(format!("USB bus {b}, port {d}")),
            _ => None,
        },
        device_node: node,
    };

    port.transport = Transport::Usb;
    port.manufacturer.clone_from(&usb.manufacturer);
    port.description = usb
        .product
        .clone()
        .or_else(|| Some(format!("USB serial device ({vid:04X}:{pid:04X})")));
    port.system.driver = link_name(&device_dir.join("driver"))
        .or_else(|| interface_dir.and_then(|i| link_name(&i.join("driver"))));
    port.system.enumerator = Some("usb".into());
    port.system.parent_instance_id = Some(sys_display(sys, usb_dir));
    port.system.location_info.clone_from(&usb.location_label);
    port.system
        .hardware_ids
        .insert(0, format!("USB\\VID_{vid:04X}&PID_{pid:04X}"));
    if let Some(speed) = read_attr(usb_dir, "speed") {
        port.extra
            .push(Property::new("USB speed", format!("{speed} Mbit/s")));
    }
    if let Some(version) = read_attr(usb_dir, "version") {
        port.extra.push(Property::new("USB version", version));
    }
    if name.starts_with("ttyACM") {
        port.extra.push(Property::new("Class", "USB CDC-ACM"));
    }
    port.usb = Some(usb);
}

fn inspect_virtual_tty(
    name: &str,
    tty_dir: &Path,
    mut port: DiscoveredPort,
) -> Option<DiscoveredPort> {
    if let Some(index) = name.strip_prefix("rfcomm") {
        let channel = read_attr(tty_dir, "channel").and_then(|c| c.parse().ok());
        port.transport = Transport::Bluetooth;
        port.description = Some("Bluetooth RFCOMM serial port".into());
        port.bluetooth = Some(BluetoothInfo {
            address: read_attr(tty_dir, "address"),
            device_name: None,
            direction: None,
            service: Some("Serial Port (RFCOMM)".into()),
            service_key: channel.filter(|c| *c != 1).map(|c: u8| format!("ch{c}")),
            channel,
        });
        port.system.driver = Some("rfcomm".into());
        port.system.enumerator = Some("bluetooth".into());
        port.extra
            .push(Property::new("RFCOMM device", format!("rfcomm{index}")));
        return Some(port);
    }
    if let Some(index) = name.strip_prefix("tnt").and_then(|n| n.parse::<u32>().ok()) {
        port.transport = Transport::Virtual;
        port.description = Some("tty0tty virtual null-modem port".into());
        port.virtual_port = Some(VirtualInfo {
            provider: "tty0tty".into(),
            detail: Some(format!("Paired with /dev/tnt{}", index ^ 1)),
            heuristic: false,
        });
        port.system.driver = Some("tty0tty".into());
        return Some(port);
    }
    None
}

#[cfg(test)]
mod tests;
