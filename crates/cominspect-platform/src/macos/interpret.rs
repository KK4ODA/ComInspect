//! Pure interpretation of raw IOKit data (compiled and tested everywhere).

use cominspect_core::model::{
    BluetoothDirection, BluetoothInfo, DiscoveredPort, InstanceIdentity, InstanceQuality,
    PlatformCapabilities, Presence, Property, ScanResult, SystemInfo, Transport, UsbInfo,
};

/// Properties of one `IOSerialBSDClient` service and its ancestry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawMacService {
    pub callout_device: Option<String>,
    pub dialin_device: Option<String>,
    pub tty_base_name: Option<String>,
    pub tty_suffix: Option<String>,
    pub tty_device: Option<String>,
    pub registry_path: Option<String>,
    /// IOKit class names from the service's provider upwards.
    pub class_chain: Vec<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub usb_serial: Option<String>,
    pub usb_vendor_name: Option<String>,
    pub usb_product_name: Option<String>,
    pub location_id: Option<u32>,
    pub interface_number: Option<u8>,
    pub bcd_device: Option<u16>,
}

pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        remembers_absent_devices: false,
        reserved_numbers: false,
        os_timestamps: false,
        monitoring: "Device-node polling".into(),
    }
}

/// Ports macOS creates for itself.
const SYSTEM_PORTS: &[&str] = &[
    "debug-console",
    "wlan-debug",
    "BLTH",
    "Bluetooth-Incoming-Port",
];

fn chain_has(raw: &RawMacService, needle: &str) -> bool {
    raw.class_chain.iter().any(|c| {
        c.to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
    })
}

pub fn classify(raw: &RawMacService) -> Transport {
    let device = raw.tty_device.as_deref().unwrap_or_default();
    if device.contains("Bluetooth") || chain_has(raw, "bluetooth") {
        return Transport::Bluetooth;
    }
    if raw.vid.is_some() || chain_has(raw, "IOUSBHostDevice") || chain_has(raw, "IOUSBDevice") {
        return Transport::Usb;
    }
    if SYSTEM_PORTS.iter().any(|s| device.eq_ignore_ascii_case(s)) {
        return Transport::Builtin;
    }
    if chain_has(raw, "IOPCIDevice") {
        return Transport::Pci;
    }
    Transport::Unknown
}

pub fn interpret_service(raw: &RawMacService) -> Option<DiscoveredPort> {
    let port_name = raw
        .callout_device
        .clone()
        .or_else(|| raw.dialin_device.clone())?;
    let device = raw
        .tty_device
        .clone()
        .unwrap_or_else(|| port_name.trim_start_matches("/dev/cu.").to_string());
    let transport = classify(raw);

    let usb = match (transport, raw.vid, raw.pid) {
        (Transport::Usb, Some(vid), Some(pid)) => Some(UsbInfo {
            vid,
            pid,
            serial_number: raw.usb_serial.clone().filter(|s| !s.trim().is_empty()),
            interface_number: raw.interface_number.or(Some(0)),
            interface_name: None,
            manufacturer: raw.usb_vendor_name.clone(),
            product: raw.usb_product_name.clone(),
            revision: raw.bcd_device,
            location: raw.location_id.map(|l| format!("0x{l:08x}")),
            location_label: raw.location_id.map(|l| format!("USB location 0x{l:08x}")),
            device_node: None,
        }),
        _ => None,
    };

    let is_system = SYSTEM_PORTS.iter().any(|s| device.eq_ignore_ascii_case(s));
    let bluetooth = (transport == Transport::Bluetooth).then(|| BluetoothInfo {
        address: None,
        device_name: (!is_system).then(|| device.clone()),
        direction: Some(if device.contains("Incoming") {
            BluetoothDirection::Incoming
        } else {
            BluetoothDirection::Outgoing
        }),
        service: Some("Serial Port (SPP)".into()),
        service_key: None,
        channel: None,
    });

    let driver = raw.class_chain.first().cloned();
    let description = match transport {
        Transport::Usb => raw
            .usb_product_name
            .clone()
            .or_else(|| Some(format!("USB serial device ({device})"))),
        Transport::Bluetooth if is_system => Some("Bluetooth incoming serial port".into()),
        Transport::Bluetooth => Some(format!("Bluetooth serial port ({device})")),
        Transport::Builtin => Some(format!("macOS system port ({device})")),
        _ => Some(format!("Serial port ({device})")),
    };

    let mut notes = Vec::new();
    if is_system {
        notes.push("macOS system port".to_string());
    }
    let mut extra = Vec::new();
    if !raw.class_chain.is_empty() {
        extra.push(Property::new(
            "IOKit classes",
            raw.class_chain
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" → "),
        ));
    }
    if let Some(base) = &raw.tty_base_name {
        extra.push(Property::new("TTY base name", base.clone()));
    }

    let instance_identity = match transport {
        Transport::Builtin | Transport::Pci => Some(InstanceIdentity {
            value: format!("macos:{device}"),
            quality: InstanceQuality::Stable,
        }),
        _ => None,
    };

    Some(DiscoveredPort {
        aliases: raw.dialin_device.iter().cloned().collect(),
        presence: Presence::Present,
        transport,
        friendly_name: None,
        description,
        manufacturer: raw.usb_vendor_name.clone(),
        usb,
        bluetooth,
        virtual_port: None,
        system: SystemInfo {
            instance_id: raw.registry_path.clone(),
            device_class: Some("IOSerialBSDClient".into()),
            enumerator: Some("IOKit".into()),
            driver,
            device_path: raw.registry_path.clone(),
            ..Default::default()
        },
        instance_identity,
        notes,
        extra,
        port_name,
        ..Default::default()
    })
}

pub fn interpret(
    raw: Vec<RawMacService>,
    scanned_at: i64,
    duration_ms: u64,
    warnings: Vec<String>,
) -> ScanResult {
    let mut ports: Vec<DiscoveredPort> = raw.iter().filter_map(interpret_service).collect();
    ports.sort_by_key(|p| cominspect_core::model::natural_sort_key(&p.port_name));
    ScanResult {
        platform: "macos".into(),
        scanned_at,
        duration_ms,
        ports,
        reserved: None,
        warnings,
        capabilities: capabilities(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cominspect_core::identity::derive_keys;

    fn ftdi() -> RawMacService {
        RawMacService {
            callout_device: Some("/dev/cu.usbserial-A10XYZ12".into()),
            dialin_device: Some("/dev/tty.usbserial-A10XYZ12".into()),
            tty_base_name: Some("usbserial".into()),
            tty_suffix: Some("A10XYZ12".into()),
            tty_device: Some("usbserial-A10XYZ12".into()),
            registry_path: Some(
                "IOService:/AppleARMPE/arm-io/AppleT8103IO/usb-drd0@2280000/.../IOSerialBSDClient"
                    .into(),
            ),
            class_chain: vec![
                "AppleUSBFTDI".into(),
                "IOUSBHostInterface".into(),
                "IOUSBHostDevice".into(),
            ],
            vid: Some(0x0403),
            pid: Some(0x6001),
            usb_serial: Some("A10XYZ12".into()),
            usb_vendor_name: Some("FTDI".into()),
            usb_product_name: Some("FT232R USB UART".into()),
            location_id: Some(0x0110_0000),
            interface_number: Some(0),
            bcd_device: Some(0x0600),
        }
    }

    #[test]
    fn usb_port() {
        let p = interpret_service(&ftdi()).unwrap();
        assert_eq!(p.port_name, "/dev/cu.usbserial-A10XYZ12");
        assert_eq!(p.aliases, vec!["/dev/tty.usbserial-A10XYZ12".to_string()]);
        assert_eq!(p.transport, Transport::Usb);
        let usb = p.usb.as_ref().unwrap();
        assert_eq!(usb.location.as_deref(), Some("0x01100000"));
        assert_eq!(p.system.driver.as_deref(), Some("AppleUSBFTDI"));
        assert_eq!(p.device_label(), "FT232R USB UART");
        // Same canonical key as on Windows and Linux.
        assert_eq!(derive_keys(&p)[0].value, "0403:6001:A10XYZ12:if0");
    }

    #[test]
    fn bluetooth_and_system_ports() {
        let incoming = RawMacService {
            callout_device: Some("/dev/cu.Bluetooth-Incoming-Port".into()),
            tty_device: Some("Bluetooth-Incoming-Port".into()),
            class_chain: vec!["IOBluetoothSerialClient".into()],
            ..Default::default()
        };
        let p = interpret_service(&incoming).unwrap();
        assert_eq!(p.transport, Transport::Bluetooth);
        assert_eq!(
            p.bluetooth.as_ref().unwrap().direction,
            Some(BluetoothDirection::Incoming)
        );
        assert_eq!(p.notes, vec!["macOS system port".to_string()]);

        let console = RawMacService {
            callout_device: Some("/dev/cu.debug-console".into()),
            tty_device: Some("debug-console".into()),
            ..Default::default()
        };
        let p = interpret_service(&console).unwrap();
        assert_eq!(p.transport, Transport::Builtin);
        assert_eq!(
            p.instance_identity.as_ref().unwrap().value,
            "macos:debug-console"
        );

        let spp = RawMacService {
            callout_device: Some("/dev/cu.HC-05-DevB".into()),
            tty_device: Some("HC-05-DevB".into()),
            class_chain: vec!["IOBluetoothSerialClientModemStreams".into()],
            ..Default::default()
        };
        let p = interpret_service(&spp).unwrap();
        assert_eq!(p.transport, Transport::Bluetooth);
        assert_eq!(
            p.bluetooth.as_ref().unwrap().device_name.as_deref(),
            Some("HC-05-DevB")
        );
        assert!(derive_keys(&p).is_empty(), "matched by port name only");
    }

    #[test]
    fn scan_sorting() {
        let mut second = ftdi();
        second.callout_device = Some("/dev/cu.usbmodem14101".into());
        let scan = interpret(vec![ftdi(), second], 1, 2, vec![]);
        assert_eq!(scan.ports[0].port_name, "/dev/cu.usbmodem14101");
        assert_eq!(scan.platform, "macos");
    }
}
