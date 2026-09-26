//! End-to-end tests of the inventory: simulated plug/unplug/renumber
//! sequences, merges and export/import between two "computers".

use cominspect_core::analysis::Severity;
use cominspect_core::export::ImportMode;
use cominspect_core::identity::MatchBasis;
use cominspect_core::model::{
    DiscoveredPort, InstanceIdentity, InstanceQuality, OsTimestamps, Presence, ScanResult,
    Transport, UsbInfo,
};
use cominspect_core::user::{CatStatus, IdentityPatch, Purpose};

use crate::view::{InventoryEventKind, PortStatus};
use crate::{Inventory, Store};

fn usb(
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
        description: Some(format!("USB device {vid:04X}:{pid:04X}")),
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

fn absent(mut port: DiscoveredPort) -> DiscoveredPort {
    port.presence = Presence::Absent;
    port
}

fn scan(platform: &str, at: i64, ports: Vec<DiscoveredPort>) -> ScanResult {
    ScanResult {
        platform: platform.into(),
        scanned_at: at,
        ports,
        ..Default::default()
    }
}

fn inventory(platform: &str) -> Inventory {
    Inventory::new(Store::open_in_memory().unwrap(), platform)
}

fn nickname(value: &str) -> IdentityPatch {
    IdentityPatch {
        nickname: Some(Some(value.into())),
        ..Default::default()
    }
}

const MIN: i64 = 60_000;

#[test]
fn device_lifecycle_with_renumbering() {
    let mut inv = inventory("windows");
    let enhanced = |port: &str| usb(port, 0x10C4, 0xEA70, Some("01A2B3C4"), Some(0), "LOC1");
    let standard = |port: &str| usb(port, 0x10C4, 0xEA70, Some("01A2B3C4"), Some(1), "LOC1");

    // 1. First scan: both CP2105 ports are new.
    let out = inv
        .reconcile(scan(
            "windows",
            MIN,
            vec![enhanced("COM5"), standard("COM6")],
        ))
        .unwrap();
    assert_eq!(out.events.len(), 2);
    assert!(
        out.events
            .iter()
            .all(|e| e.kind == InventoryEventKind::NewDevice && e.initial)
    );
    let view = inv.view().unwrap();
    assert_eq!(view.rows.len(), 2);
    assert_eq!(view.summary.connected, 2);
    let enhanced_id = view
        .rows
        .iter()
        .find(|r| r.port.as_deref() == Some("COM5"))
        .unwrap()
        .device_id;
    assert_eq!(
        view.rows
            .iter()
            .find(|r| r.device_id == enhanced_id)
            .unwrap()
            .hint_label
            .as_deref(),
        Some("Enhanced COM port")
    );

    inv.update_identity(
        enhanced_id,
        &IdentityPatch {
            nickname: Some(Some("FTDX10 CAT Enhanced".into())),
            equipment: Some(Some("Yaesu FTDX10".into())),
            purpose: Some(Some(Purpose::Cat)),
            cat_status: Some(CatStatus::Verified),
            ..Default::default()
        },
    )
    .unwrap();

    // 2. Enhanced port renumbered to COM7; Standard unplugged but remembered
    //    by Windows as a hidden device.
    let out = inv
        .reconcile(scan(
            "windows",
            2 * MIN,
            vec![enhanced("COM7"), absent(standard("COM6"))],
        ))
        .unwrap();
    let kinds: Vec<_> = out
        .events
        .iter()
        .map(|e| (e.kind, e.port.clone(), e.previous_port.clone()))
        .collect();
    assert!(kinds.contains(&(
        InventoryEventKind::PortChanged,
        Some("COM7".into()),
        Some("COM5".into())
    )));
    assert!(kinds.contains(&(InventoryEventKind::Disconnected, Some("COM6".into()), None)));
    assert!(out.events.iter().all(|e| !e.initial));
    let changed = out
        .events
        .iter()
        .find(|e| e.kind == InventoryEventKind::PortChanged)
        .unwrap();
    assert_eq!(changed.label, "FTDX10 CAT Enhanced");

    let view = inv.view().unwrap();
    let row = view
        .rows
        .iter()
        .find(|r| r.device_id == enhanced_id)
        .unwrap();
    assert_eq!(row.status, PortStatus::Connected);
    assert_eq!(row.port.as_deref(), Some("COM7"));
    assert_eq!(row.previous_port.as_deref(), Some("COM5"));
    assert_eq!(row.port_changed_at, Some(2 * MIN));
    assert_eq!(row.cat_status, CatStatus::Verified);
    let std_row = view
        .rows
        .iter()
        .find(|r| r.device_id != enhanced_id)
        .unwrap();
    assert_eq!(std_row.status, PortStatus::AbsentOs);

    // 3. Everything unplugged and gone (Linux/macOS behaviour).
    let out = inv.reconcile(scan("windows", 3 * MIN, vec![])).unwrap();
    assert_eq!(out.events.len(), 1);
    assert_eq!(out.events[0].kind, InventoryEventKind::Disconnected);
    let view = inv.view().unwrap();
    assert!(view.rows.iter().all(|r| r.status == PortStatus::Absent));
    let row = view
        .rows
        .iter()
        .find(|r| r.device_id == enhanced_id)
        .unwrap();
    assert_eq!(row.last_seen, Some(2 * MIN));

    // 4. Reconnected on yet another COM number.
    let out = inv
        .reconcile(scan("windows", 10 * MIN, vec![enhanced("COM9")]))
        .unwrap();
    assert_eq!(out.events[0].kind, InventoryEventKind::Connected);
    assert_eq!(out.events[0].previous_port.as_deref(), Some("COM7"));

    let detail = inv.detail(enhanced_id).unwrap();
    assert_eq!(detail.row.nickname.as_deref(), Some("FTDX10 CAT Enhanced"));
    assert_eq!(detail.recognized_by, Some(MatchBasis::UsbSerial));
    assert!(detail.identity_note.contains("USB serial number"));
    let ports: Vec<_> = detail
        .port_history
        .iter()
        .map(|p| (p.port.as_str(), p.current))
        .collect();
    assert_eq!(
        ports,
        vec![("COM9", true), ("COM7", false), ("COM5", false)]
    );
    assert!(detail.events.len() >= 3);
    assert!(detail.snapshot_live);
    assert_eq!(detail.hints[0].id, "silabs-cp2105-enhanced");
}

#[test]
fn serial_less_device_can_be_linked_across_sockets() {
    let mut inv = inventory("linux");
    let ch340 = |port: &str, loc: &str| usb(port, 0x1A86, 0x7523, None, None, loc);

    inv.reconcile(scan(
        "linux",
        MIN,
        vec![ch340("/dev/ttyUSB0", "pci-0000:00:14.0-usb-0:1")],
    ))
    .unwrap();
    let a = inv.view().unwrap().rows[0].device_id;
    inv.update_identity(a, &nickname("TS-480 Programming Cable"))
        .unwrap();

    // Moved to another socket: indistinguishable from another CH340 → new entry.
    inv.reconcile(scan(
        "linux",
        2 * MIN,
        vec![ch340("/dev/ttyUSB0", "pci-0000:00:14.0-usb-0:2")],
    ))
    .unwrap();
    let view = inv.view().unwrap();
    assert_eq!(view.rows.len(), 2);
    let b = view
        .rows
        .iter()
        .find(|r| r.device_id != a)
        .unwrap()
        .device_id;

    let detail = inv.detail(a).unwrap();
    assert!(detail.identity_note.contains("no unique serial number"));
    assert_eq!(detail.merge_candidates.len(), 1);
    assert_eq!(detail.merge_candidates[0].device_id, b);

    inv.merge(a, b).unwrap();
    let view = inv.view().unwrap();
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].device_id, a);
    assert_eq!(
        view.rows[0].status,
        PortStatus::Connected,
        "live state carried over"
    );

    // Recognized in either socket from now on.
    for (i, loc) in ["pci-0000:00:14.0-usb-0:1", "pci-0000:00:14.0-usb-0:2"]
        .iter()
        .enumerate()
    {
        inv.reconcile(scan(
            "linux",
            (10 + i as i64) * MIN,
            vec![ch340("/dev/ttyUSB3", loc)],
        ))
        .unwrap();
        let view = inv.view().unwrap();
        assert_eq!(view.rows.len(), 1, "socket {loc}");
        assert_eq!(
            view.rows[0].nickname.as_deref(),
            Some("TS-480 Programming Cable")
        );
    }
    let events = inv.detail(a).unwrap().events;
    assert!(events.iter().any(|e| e.kind == "merged"));
}

#[test]
fn merge_refuses_two_connected_devices() {
    let mut inv = inventory("linux");
    inv.reconcile(scan(
        "linux",
        MIN,
        vec![
            usb("/dev/ttyUSB0", 0x1A86, 0x7523, None, None, "L1"),
            usb("/dev/ttyUSB1", 0x1A86, 0x7523, None, None, "L2"),
        ],
    ))
    .unwrap();
    let view = inv.view().unwrap();
    let err = inv
        .merge(view.rows[0].device_id, view.rows[1].device_id)
        .unwrap_err();
    assert!(err.to_string().contains("Both devices are connected"));
    assert!(
        inv.detail(view.rows[0].device_id)
            .unwrap()
            .merge_candidates
            .is_empty()
    );
}

#[test]
fn identities_travel_to_another_computer() {
    // Computer A (Windows).
    let mut a = inventory("windows");
    a.reconcile(scan(
        "windows",
        MIN,
        vec![
            usb(
                "COM7",
                0x10C4,
                0xEA70,
                Some("01A2B3C4"),
                Some(0),
                "PCIROOT(0)#USB(2)",
            ),
            usb("COM4", 0x1A86, 0x7523, None, None, "PCIROOT(0)#USB(3)"),
        ],
    ))
    .unwrap();
    for row in a.view().unwrap().rows {
        let name = if row.vid == Some(0x10C4) {
            "FTDX10 CAT Enhanced"
        } else {
            "Programming cable"
        };
        a.update_identity(row.device_id, &nickname(name)).unwrap();
    }
    let doc = a.export("0.1.0", Some("SHACK-PC")).unwrap();
    assert_eq!(doc.devices.len(), 2);
    let json = doc.to_json_pretty();
    let doc = cominspect_core::export::ExportDocument::from_json(&json).unwrap();

    // Computer B (Linux) imports the file.
    let mut b = inventory("linux");
    let report = b
        .import(&doc, ImportMode::Merge, Some("shack-laptop"))
        .unwrap();
    assert_eq!(
        (report.created, report.matched, report.same_machine),
        (2, 0, false)
    );
    let view = b.view().unwrap();
    assert!(view.rows.iter().all(|r| r.status == PortStatus::Awaiting));
    assert!(
        view.rows.iter().all(|r| r.port.is_none()),
        "COM numbers are not applied elsewhere"
    );
    assert_eq!(view.summary.awaiting, 2);

    // The radio appears with a completely different port name and is
    // recognized by its portable USB serial key.
    b.reconcile(scan(
        "linux",
        5 * MIN,
        vec![
            usb(
                "/dev/ttyUSB0",
                0x10C4,
                0xEA70,
                Some("01A2B3C4"),
                Some(0),
                "pci-0000:00:14.0-usb-0:4",
            ),
            usb(
                "/dev/ttyUSB1",
                0x1A86,
                0x7523,
                None,
                None,
                "pci-0000:00:14.0-usb-0:5",
            ),
        ],
    ))
    .unwrap();
    let view = b.view().unwrap();
    assert_eq!(
        view.rows.len(),
        3,
        "the serial-less cable cannot be matched across computers"
    );
    let radio = view
        .rows
        .iter()
        .find(|r| r.port.as_deref() == Some("/dev/ttyUSB0"))
        .unwrap();
    assert_eq!(radio.nickname.as_deref(), Some("FTDX10 CAT Enhanced"));
    assert_eq!(radio.status, PortStatus::Connected);
    let detail = b.detail(radio.device_id).unwrap();
    assert_eq!(
        detail.imported_from.unwrap().last_port.as_deref(),
        Some("COM7")
    );
    assert!(view.rows.iter().any(|r| r.status == PortStatus::Awaiting));

    // Importing again is idempotent.
    let report = b
        .import(&doc, ImportMode::Merge, Some("shack-laptop"))
        .unwrap();
    assert_eq!((report.created, report.matched), (0, 0));
    assert_eq!(report.unchanged, 2);
}

#[test]
fn import_merge_and_overwrite_modes() {
    let mut inv = inventory("windows");
    inv.reconcile(scan(
        "windows",
        MIN,
        vec![usb("COM3", 0x0403, 0x6001, Some("A10XYZ12"), Some(0), "L")],
    ))
    .unwrap();
    let id = inv.view().unwrap().rows[0].device_id;
    inv.update_identity(id, &nickname("Local name")).unwrap();
    let mut doc = inv.export("0.1.0", Some("PC")).unwrap();
    doc.devices[0].identity.nickname = Some("Imported name".into());
    doc.devices[0].identity.equipment = Some("Kenwood TS-480".into());

    inv.import(&doc, ImportMode::Merge, Some("PC")).unwrap();
    let row = &inv.view().unwrap().rows[0];
    assert_eq!(row.nickname.as_deref(), Some("Local name"));
    assert_eq!(row.equipment.as_deref(), Some("Kenwood TS-480"));

    let report = inv.import(&doc, ImportMode::Overwrite, Some("PC")).unwrap();
    assert!(report.same_machine);
    assert_eq!(
        inv.view().unwrap().rows[0].nickname.as_deref(),
        Some("Imported name")
    );
}

#[test]
fn windows_hidden_device_uses_os_history() {
    let mut inv = inventory("windows");
    let mut ghost = absent(usb("COM16", 0x1546, 0x01A7, None, None, "L"));
    ghost.instance_identity = Some(InstanceIdentity {
        value: r"USB\VID_1546&PID_01A7\5&1234&0&2".into(),
        quality: InstanceQuality::Location,
    });
    ghost.os_times = OsTimestamps {
        last_arrival: Some(1_000),
        last_removal: Some(5_000),
        ..Default::default()
    };
    inv.reconcile(scan("windows", MIN, vec![ghost.clone()]))
        .unwrap();
    let view = inv.view().unwrap();
    let row = &view.rows[0];
    assert_eq!(row.status, PortStatus::AbsentOs);
    assert_eq!(row.last_seen, Some(5_000));
    assert_eq!(row.last_seen_source, Some(crate::view::LastSeenSource::Os));
    assert_eq!(view.summary.disconnected, 1);

    // The GPS is plugged in: same instance, now present.
    let mut present = ghost.clone();
    present.presence = Presence::Present;
    let out = inv
        .reconcile(scan("windows", 2 * MIN, vec![present]))
        .unwrap();
    assert_eq!(out.events[0].kind, InventoryEventKind::Connected);
    let row = &inv.view().unwrap().rows[0];
    assert_eq!(row.status, PortStatus::Connected);
    assert_eq!(row.last_seen_source, Some(crate::view::LastSeenSource::App));
    assert_eq!(row.hint_label, None);
    let detail = inv.detail(row.device_id).unwrap();
    assert_eq!(detail.hints[0].suggested_purpose, Some(Purpose::Gps));
}

#[test]
fn conflicts_are_attached_to_rows() {
    let mut inv = inventory("windows");
    inv.reconcile(scan(
        "windows",
        MIN,
        vec![
            usb("COM7", 0x0403, 0x6001, Some("AAAA1111"), Some(0), "L1"),
            absent(usb("COM7", 0x0403, 0x6001, Some("BBBB2222"), Some(0), "L2")),
        ],
    ))
    .unwrap();
    let view = inv.view().unwrap();
    let finding = view
        .findings
        .iter()
        .find(|f| f.finding.code == "duplicate-port")
        .unwrap();
    assert_eq!(finding.device_ids.len(), 2);
    assert!(
        view.rows
            .iter()
            .all(|r| r.severity == Some(Severity::Warning))
    );
    assert_eq!(view.summary.warnings, 2);
}

#[test]
fn unchanged_scans_do_not_rewrite_rows() {
    let mut inv = inventory("linux");
    let port = usb(
        "/dev/ttyACM0",
        0x2341,
        0x0043,
        Some("85736323838351F0B180"),
        Some(0),
        "L",
    );
    inv.reconcile(scan("linux", MIN, vec![port.clone()]))
        .unwrap();
    let updated = |inv: &Inventory| -> i64 {
        inv.store()
            .connection()
            .query_row("SELECT updated_at FROM devices", [], |r| r.get(0))
            .unwrap()
    };
    let first = updated(&inv);
    inv.reconcile(scan("linux", MIN + 5_000, vec![port.clone()]))
        .unwrap();
    assert_eq!(updated(&inv), first, "no write within the touch interval");
    inv.reconcile(scan("linux", 3 * MIN, vec![port])).unwrap();
    assert_eq!(updated(&inv), 3 * MIN);
}

#[test]
fn ignore_and_forget() {
    let mut inv = inventory("linux");
    inv.reconcile(scan(
        "linux",
        MIN,
        vec![usb(
            "/dev/ttyUSB0",
            0x0403,
            0x6001,
            Some("A10XYZ12"),
            Some(0),
            "L",
        )],
    ))
    .unwrap();
    let id = inv.view().unwrap().rows[0].device_id;
    inv.set_ignored(id, true).unwrap();
    let view = inv.view().unwrap();
    assert!(view.rows[0].ignored);
    assert_eq!((view.summary.total, view.summary.ignored), (0, 1));

    inv.forget(id).unwrap();
    assert!(inv.view().unwrap().rows.is_empty());
    assert!(inv.forget(id).is_err());
    // Seen again: a brand new record without the old identity.
    let out = inv
        .reconcile(scan(
            "linux",
            2 * MIN,
            vec![usb(
                "/dev/ttyUSB0",
                0x0403,
                0x6001,
                Some("A10XYZ12"),
                Some(0),
                "L",
            )],
        ))
        .unwrap();
    assert_eq!(out.events[0].kind, InventoryEventKind::NewDevice);
    assert!(!inv.view().unwrap().rows[0].ignored);
}

#[test]
fn persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inventory.db");
    let port = usb("COM3", 0x0403, 0x6001, Some("A10XYZ12"), Some(0), "L");
    {
        let (store, _) = Store::open(&path, "0.1.0").unwrap();
        let mut inv = Inventory::new(store, "windows");
        inv.reconcile(scan("windows", MIN, vec![port.clone()]))
            .unwrap();
        let id = inv.view().unwrap().rows[0].device_id;
        inv.update_identity(id, &nickname("IC-7300 CI-V")).unwrap();
    }
    let (store, _) = Store::open(&path, "0.1.1").unwrap();
    let mut inv = Inventory::new(store, "windows");
    // Before any scan, the device is shown as disconnected with its identity.
    let view = inv.view().unwrap();
    assert_eq!(view.rows[0].status, PortStatus::Absent);
    assert_eq!(view.rows[0].nickname.as_deref(), Some("IC-7300 CI-V"));
    // The first scan after startup reports it as reconnected (initial).
    let out = inv.reconcile(scan("windows", 2 * MIN, vec![port])).unwrap();
    assert!(
        out.events.is_empty(),
        "was connected at shutdown, still connected"
    );
    assert_eq!(inv.view().unwrap().rows[0].status, PortStatus::Connected);
}
