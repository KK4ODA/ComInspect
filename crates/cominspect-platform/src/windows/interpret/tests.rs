//! Tests with device data shaped like real Windows 10/11 output.

use std::collections::HashMap;

use cominspect_core::analysis::analyze_scan;
use cominspect_core::identity::{KeyKind, derive_keys};
use cominspect_core::model::{
    BluetoothDirection, InstanceQuality, Presence, Transport, com_number,
};

use super::*;

fn node(instance: &str) -> RawDevNode {
    RawDevNode {
        instance_id: instance.into(),
        present: true,
        enumerator: Some(enumerator_of(instance)),
        class: Some("Ports".into()),
        class_guid: Some("{4d36e978-e325-11ce-bfc1-08002be10318}".into()),
        ..Default::default()
    }
}

fn with_port(mut n: RawDevNode, port: &str, desc: &str) -> RawDevNode {
    n.port_name = Some(port.into());
    n.device_desc = Some(desc.into());
    n.friendly_name = Some(format!("{desc} ({port})"));
    n
}

fn usb_device(instance: &str, product: &str, location: &str) -> RawDevNode {
    RawDevNode {
        instance_id: instance.into(),
        present: true,
        enumerator: Some("USB".into()),
        bus_reported_desc: Some(product.into()),
        location_paths: vec![location.into()],
        location_info: Some("Port_#0002.Hub_#0001".into()),
        hardware_ids: vec![format!(
            "{}&REV_0100",
            instance.rsplit_once('\\').unwrap().0
        )],
        ..Default::default()
    }
}

fn root_hub() -> RawDevNode {
    RawDevNode {
        instance_id: r"USB\ROOT_HUB30\4&1A2B3C4D&0&0".into(),
        present: true,
        ..Default::default()
    }
}

fn host_controller() -> RawDevNode {
    RawDevNode {
        instance_id: r"PCI\VEN_8086&DEV_A36D&SUBSYS_86941043&REV_10\3&11583659&0&A0".into(),
        present: true,
        ..Default::default()
    }
}

fn sc(kernel: &str, port: &str) -> RawSerialComm {
    RawSerialComm {
        kernel_name: kernel.into(),
        port_name: port.into(),
    }
}

fn cp2105_enhanced() -> RawPort {
    let mut n = with_port(
        node(r"USB\VID_10C4&PID_EA70&MI_00\6&2A3B4C5D&0&0000"),
        "COM7",
        "Silicon Labs Dual CP2105 USB to UART Bridge: Enhanced COM Port",
    );
    n.manufacturer = Some("Silicon Labs".into());
    n.service = Some("silabser".into());
    n.driver_provider = Some("Silicon Laboratories Inc.".into());
    n.driver_version = Some("10.1.10.0".into());
    n.driver_date = Some(1_577_836_800_000);
    n.hardware_ids = vec![
        r"USB\VID_10C4&PID_EA70&REV_0100&MI_00".into(),
        r"USB\VID_10C4&PID_EA70&MI_00".into(),
    ];
    RawPort {
        node: n,
        ancestors: vec![
            usb_device(
                r"USB\VID_10C4&PID_EA70\01A2B3C4",
                "CP2105 Dual USB to UART Bridge Controller",
                "PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(2)",
            ),
            root_hub(),
            host_controller(),
        ],
    }
}

fn ftdi(present: bool) -> RawPort {
    let mut n = with_port(
        node(r"FTDIBUS\VID_0403+PID_6001+A10XYZ12A\0000"),
        "COM5",
        "USB Serial Port",
    );
    n.present = present;
    n.manufacturer = Some("FTDI".into());
    n.service = Some("FTSER2K".into());
    n.last_removal_date = (!present).then_some(1_700_000_000_000);
    RawPort {
        node: n,
        ancestors: if present {
            vec![
                usb_device(
                    r"USB\VID_0403&PID_6001\A10XYZ12",
                    "FT232R USB UART",
                    "PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(3)",
                ),
                root_hub(),
            ]
        } else {
            vec![]
        },
    }
}

fn scan_of(ports: Vec<RawPort>, serialcomm: Vec<RawSerialComm>) -> ScanResult {
    interpret(
        RawWindowsScan {
            ports,
            serialcomm,
            ..Default::default()
        },
        1,
        2,
    )
}

#[test]
fn id_parsing() {
    let ids = parse_usb_ids(r"USB\VID_10C4&PID_EA70&REV_0100&MI_01");
    assert_eq!(ids.vid, Some(0x10C4));
    assert_eq!(ids.pid, Some(0xEA70));
    assert_eq!(ids.revision, Some(0x0100));
    assert_eq!(ids.interface, Some(1));
    assert_eq!(
        parse_usb_ids("FTDIBUS\\VID_0403+PID_6010+FT1234ABB\\0000").pid,
        Some(0x6010)
    );

    assert!(is_usb_device_id(r"USB\VID_10C4&PID_EA70\01A2B3C4"));
    assert!(!is_usb_device_id(
        r"USB\VID_10C4&PID_EA70&MI_00\6&2A3B4C5D&0&0000"
    ));
    assert!(!is_usb_device_id(r"USB\ROOT_HUB30\4&1A2B3C4D&0&0"));
    assert_eq!(
        usb_serial_from_instance(r"USB\VID_10C4&PID_EA70\01A2B3C4").as_deref(),
        Some("01A2B3C4")
    );
    assert_eq!(
        usb_serial_from_instance(r"USB\VID_1A86&PID_7523\5&2A3B4C5D&0&3"),
        None
    );

    assert_eq!(
        ftdi_serial_and_channel(r"FTDIBUS\VID_0403+PID_6010+FT1234ABB\0000"),
        Some((Some("FT1234AB".into()), 1))
    );
    assert_eq!(
        ftdi_serial_and_channel(r"FTDIBUS\VID_0403+PID_6001+A10XYZ12A\0000"),
        Some((Some("A10XYZ12".into()), 0))
    );
    assert_eq!(
        ftdi_serial_and_channel(r"USB\VID_0403&PID_6001\A10XYZ12"),
        None
    );

    assert_eq!(
        bluetooth_address(
            r"BTHENUM\{00001101-0000-1000-8000-00805F9B34FB}_LOCALMFG&0002\7&2E1E2F58&0&98D331F5B4C2_C00000000"
        ),
        Some(("98D331F5B4C2".into(), Some("C00000000".into())))
    );
    assert_eq!(
        bluetooth_address(r"BTHENUM\DEV_98D331F5B4C2\7&3A5F3E1&0&BLUETOOTHDEVICE_98D331F5B4C2"),
        Some(("98D331F5B4C2".into(), None))
    );
}

#[test]
fn composite_cp2105_port() {
    let scan = scan_of(
        vec![cp2105_enhanced()],
        vec![sc(r"\Device\Silabser0", "COM7")],
    );
    assert_eq!(scan.ports.len(), 1);
    let p = &scan.ports[0];
    assert_eq!(p.port_name, "COM7");
    assert_eq!(p.transport, Transport::Usb);
    assert_eq!(p.presence, Presence::Present);
    let usb = p.usb.as_ref().unwrap();
    assert_eq!((usb.vid, usb.pid), (0x10C4, 0xEA70));
    assert_eq!(usb.serial_number.as_deref(), Some("01A2B3C4"));
    assert_eq!(usb.interface_number, Some(0));
    assert_eq!(usb.revision, Some(0x0100));
    assert_eq!(
        usb.product.as_deref(),
        Some("CP2105 Dual USB to UART Bridge Controller")
    );
    assert_eq!(
        usb.location.as_deref(),
        Some("PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(2)")
    );
    assert_eq!(
        usb.device_node.as_deref(),
        Some(r"USB\VID_10C4&PID_EA70\01A2B3C4")
    );
    assert_eq!(p.system.kernel_name.as_deref(), Some(r"\Device\Silabser0"));
    assert_eq!(p.system.driver.as_deref(), Some("silabser"));
    assert_eq!(p.system.driver_date.as_deref(), Some("2020-01-01"));
    assert_eq!(
        p.instance_identity.as_ref().unwrap().quality,
        InstanceQuality::DeviceUnique
    );
    assert_eq!(
        p.device_label(),
        "Silicon Labs Dual CP2105 USB to UART Bridge: Enhanced COM Port"
    );
    assert_eq!(derive_keys(p)[0].value, "10C4:EA70:01A2B3C4:if0");
    assert!(scan.capabilities.remembers_absent_devices);
}

#[test]
fn ftdi_present_and_hidden_share_identity() {
    let present = scan_of(vec![ftdi(true)], vec![sc(r"\Device\VCP0", "COM5")]);
    let p = &present.ports[0];
    let usb = p.usb.as_ref().unwrap();
    assert_eq!(usb.serial_number.as_deref(), Some("A10XYZ12"));
    assert_eq!(usb.interface_number, Some(0));
    assert_eq!(
        p.device_label(),
        "FT232R USB UART",
        "generic INF description replaced"
    );

    let hidden = scan_of(vec![ftdi(false)], vec![]);
    let h = &hidden.ports[0];
    assert_eq!(h.presence, Presence::Absent);
    assert_eq!(
        h.usb.as_ref().unwrap().serial_number.as_deref(),
        Some("A10XYZ12")
    );
    assert_eq!(h.os_times.last_removal, Some(1_700_000_000_000));
    // Same strong keys whether present or hidden.
    assert_eq!(derive_keys(p)[0], derive_keys(h)[0]);
    assert_eq!(derive_keys(h)[0].kind, KeyKind::UsbSerial);
}

#[test]
fn serial_less_phantom_uses_friendly_name_and_location_quality() {
    let mut n = node(r"USB\VID_1A86&PID_7523\5&2A3B4C5D&0&3");
    n.present = false;
    n.friendly_name = Some("USB-SERIAL CH340 (COM4)".into());
    n.device_desc = Some("USB-SERIAL CH340".into());
    n.last_arrival_date = Some(5);
    let scan = scan_of(
        vec![RawPort {
            node: n,
            ancestors: vec![],
        }],
        vec![],
    );
    let p = &scan.ports[0];
    assert_eq!(p.port_name, "COM4");
    assert_eq!(p.presence, Presence::Absent);
    assert_eq!(p.usb.as_ref().unwrap().serial_number, None);
    assert_eq!(
        p.instance_identity.as_ref().unwrap().quality,
        InstanceQuality::Location
    );
    let keys = derive_keys(p);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].kind, KeyKind::OsDevice);
}

#[test]
fn prolific_problem_code() {
    let mut n = with_port(
        node(r"USB\VID_067B&PID_2303\5&11AA22BB&0&1"),
        "COM3",
        "Prolific USB-to-Serial Comm Port",
    );
    n.problem = Some(10);
    let scan = scan_of(
        vec![RawPort {
            node: n,
            ancestors: vec![],
        }],
        vec![],
    );
    let problem = scan.ports[0].system.problem.as_ref().unwrap();
    assert_eq!(problem.code, 10);
    assert!(problem.description.ends_with("(Code 10)"));
    let findings = analyze_scan(&scan);
    assert!(
        findings
            .iter()
            .any(|f| f.code == "device-problem" && f.detail.contains("PL2303"))
    );
}

#[test]
fn bluetooth_ports() {
    let mut names = HashMap::new();
    names.insert("98D331F5B4C2".to_string(), "HC-05".to_string());
    let out = with_port(
        node(
            r"BTHENUM\{00001101-0000-1000-8000-00805F9B34FB}_LOCALMFG&0002\7&2E1E2F58&0&98D331F5B4C2_C00000000",
        ),
        "COM20",
        "Standard Serial over Bluetooth link",
    );
    let inc = with_port(
        node(
            r"BTHENUM\{00001101-0000-1000-8000-00805F9B34FB}_LOCALMFG&0000\7&2E1E2F58&0&000000000000_00000000",
        ),
        "COM21",
        "Standard Serial over Bluetooth link",
    );
    let scan = interpret(
        RawWindowsScan {
            ports: vec![
                RawPort {
                    node: out,
                    ancestors: vec![],
                },
                RawPort {
                    node: inc,
                    ancestors: vec![],
                },
            ],
            serialcomm: vec![
                sc(r"\Device\BthModem0", "COM20"),
                sc(r"\Device\BthModem1", "COM21"),
            ],
            bluetooth_names: names,
            ..Default::default()
        },
        0,
        0,
    );
    let out = &scan.ports[0];
    assert_eq!(out.transport, Transport::Bluetooth);
    let bt = out.bluetooth.as_ref().unwrap();
    assert_eq!(bt.address.as_deref(), Some("98:D3:31:F5:B4:C2"));
    assert_eq!(bt.device_name.as_deref(), Some("HC-05"));
    assert_eq!(bt.direction, Some(BluetoothDirection::Outgoing));
    assert_eq!(bt.service.as_deref(), Some("Serial Port (SPP)"));
    assert_eq!(bt.service_key, None);
    assert_eq!(derive_keys(out)[0].value, "98:D3:31:F5:B4:C2");

    let inc = &scan.ports[1];
    let bt = inc.bluetooth.as_ref().unwrap();
    assert_eq!(bt.address, None);
    assert_eq!(bt.direction, Some(BluetoothDirection::Incoming));
    assert_eq!(
        inc.instance_identity.as_ref().unwrap().quality,
        InstanceQuality::Stable
    );
}

#[test]
fn builtin_pci_and_non_com_ports() {
    let com1 = with_port(node(r"ACPI\PNP0501\1"), "COM1", "Communications Port");
    let amt = with_port(
        node(r"PCI\VEN_8086&DEV_A13D&SUBSYS_86941043&REV_31\3&11583659&0&B3"),
        "COM3",
        "Intel(R) Active Management Technology - SOL",
    );
    let lpt = with_port(node(r"ACPI\PNP0400\0"), "LPT1", "Printer Port");
    let scan = scan_of(
        vec![
            RawPort {
                node: com1,
                ancestors: vec![],
            },
            RawPort {
                node: amt,
                ancestors: vec![],
            },
            RawPort {
                node: lpt,
                ancestors: vec![],
            },
        ],
        vec![sc(r"\Device\Serial0", "COM1")],
    );
    assert_eq!(scan.ports.len(), 2, "LPT ports are not serial ports");
    assert_eq!(scan.ports[0].transport, Transport::Builtin);
    assert_eq!(
        scan.ports[0].instance_identity.as_ref().unwrap().quality,
        InstanceQuality::Stable
    );
    assert_eq!(scan.ports[1].transport, Transport::Pci);
    assert_eq!(
        scan.ports[1].device_label(),
        "Intel(R) Active Management Technology - SOL"
    );
}

#[test]
fn com0com_pairs_and_serialcomm_only_ports() {
    let root = RawDevNode {
        instance_id: r"ROOT\COM0COM\0000".into(),
        present: true,
        ..Default::default()
    };
    let a = with_port(
        node(r"COM0COM\PORT\CNCA0"),
        "COM12",
        "com0com - serial port emulator",
    );
    let b = with_port(
        node(r"COM0COM\PORT\CNCB0"),
        "COM13",
        "com0com - serial port emulator",
    );
    let scan = scan_of(
        vec![
            RawPort {
                node: a,
                ancestors: vec![root.clone()],
            },
            RawPort {
                node: b,
                ancestors: vec![root],
            },
        ],
        vec![
            sc(r"\Device\com0com10", "COM12"),
            sc(r"\Device\com0com20", "COM13"),
            sc(r"\Device\VSPE1", "COM30"),
            sc(r"\Device\evserial7", "COM31"),
        ],
    );
    let names: Vec<_> = scan.ports.iter().map(|p| p.port_name.as_str()).collect();
    assert_eq!(names, vec!["COM12", "COM13", "COM30", "COM31"]);
    let a = &scan.ports[0];
    assert_eq!(a.transport, Transport::Virtual);
    let v = a.virtual_port.as_ref().unwrap();
    assert_eq!(v.provider, "com0com");
    assert!(!v.heuristic);
    assert_eq!(v.detail.as_deref(), Some("Paired with COM13 (CNCB0)"));

    let vspe = &scan.ports[2];
    assert_eq!(vspe.transport, Transport::Virtual);
    assert_eq!(vspe.virtual_port.as_ref().unwrap().provider, "VSPE");
    assert!(vspe.virtual_port.as_ref().unwrap().heuristic);
    assert_eq!(vspe.system.kernel_name.as_deref(), Some(r"\Device\VSPE1"));
    assert_eq!(
        scan.ports[3].virtual_port.as_ref().unwrap().provider,
        "Eltima Virtual Serial Port"
    );
    assert_eq!(derive_keys(vspe).last().unwrap().value, "vspe:COM30");
}

#[test]
fn comdb_and_conflicts() {
    let mut hidden = ftdi(false);
    hidden.node.port_name = Some("COM7".into());
    let scan = interpret(
        RawWindowsScan {
            ports: vec![cp2105_enhanced(), hidden],
            serialcomm: vec![sc(r"\Device\Silabser0", "COM7")],
            comdb: Some(RawComDb {
                bitmap: vec![0b0100_0101, 0b0000_0000],
                database_size: 16,
                source: "ComDB API".into(),
            }),
            ..Default::default()
        },
        0,
        0,
    );
    assert_eq!(scan.reserved.as_ref().unwrap().numbers, vec![1, 3, 7]);
    assert_eq!(scan.ports.len(), 2);
    assert!(scan.ports[0].is_present(), "present port sorts first");
    let findings = analyze_scan(&scan);
    let codes: Vec<_> = findings.iter().map(|f| f.code.as_str()).collect();
    assert!(codes.contains(&"duplicate-port"));
    assert!(codes.contains(&"stale-reservation"));
    assert!(
        scan.ports
            .iter()
            .all(|p| com_number(&p.port_name) == Some(7))
    );
}

/// VSPE's ports sit on its own "Eterlogic Virtual Serial Bus". Whatever
/// that bus is called, known virtual-port software is recognized as virtual,
/// so the app can show how VSPE links the port.
#[test]
fn virtual_ports_on_an_unfamiliar_bus() {
    let mut port = with_port(
        node(r"VSPEBUS\PORT\1"),
        "COM21",
        "Eterlogic Virtual Serial Port",
    );
    port.manufacturer = Some("Eterlogic".into());
    let mut bus = node(r"SWD\VSPE\0");
    bus.enumerator = Some("VSPEBUS".into());
    let unknown = with_port(node(r"MYSTERY\PORT\1"), "COM40", "Mystery Port");
    let scan = scan_of(
        vec![
            RawPort {
                node: port,
                ancestors: vec![bus],
            },
            RawPort {
                node: unknown,
                ancestors: vec![],
            },
        ],
        vec![],
    );
    let vspe = &scan.ports[0];
    assert_eq!(vspe.transport, Transport::Virtual);
    assert_eq!(vspe.virtual_port.as_ref().unwrap().provider, "VSPE");
    // Unknown software on an unknown bus stays unknown.
    assert_eq!(scan.ports[1].transport, Transport::Unknown);
}
