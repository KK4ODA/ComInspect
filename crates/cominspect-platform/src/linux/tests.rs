//! Fixture-based tests: a fake sysfs tree mirroring real kernel layouts.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use cominspect_core::identity::{KeyKind, derive_keys};
use cominspect_core::model::{DiscoveredPort, InstanceQuality, Transport};

use super::discover_at;

struct Fixture {
    _root: tempfile::TempDir,
    sys: PathBuf,
    dev: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let sys = root.path().join("sys");
        let dev = root.path().join("dev");
        for d in [
            "class/tty",
            "bus/usb/drivers/ftdi_sio",
            "bus/usb/drivers/cp210x",
            "bus/usb/drivers/cdc_acm",
            "bus/usb-serial/drivers/ftdi_sio",
            "bus/usb-serial/drivers/cp210x",
            "bus/pnp/drivers/serial",
            "bus/pci/drivers/serial",
            "bus/serial-base/drivers/port",
            "bus/platform",
            "devices/virtual/tty",
        ] {
            fs::create_dir_all(sys.join(d)).unwrap();
        }
        fs::create_dir_all(dev.join("serial/by-id")).unwrap();
        fs::create_dir_all(dev.join("serial/by-path")).unwrap();
        Fixture {
            _root: root,
            sys,
            dev,
        }
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let p = self.sys.join(rel);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn attrs(&self, rel: &str, attrs: &[(&str, &str)]) -> PathBuf {
        let dir = self.dir(rel);
        for (name, value) in attrs {
            fs::write(dir.join(name), format!("{value}\n")).unwrap();
        }
        dir
    }

    fn link(&self, from: &Path, to_rel: &str) {
        symlink(self.sys.join(to_rel), from).unwrap();
    }

    /// Registers a tty class device living in `tty_rel` whose `device` link
    /// points at `device_rel` (if any).
    fn tty(&self, name: &str, tty_rel: &str, device_rel: Option<&str>, attrs: &[(&str, &str)]) {
        let tty_dir = self.attrs(tty_rel, attrs);
        if let Some(device) = device_rel {
            self.link(&tty_dir.join("device"), device);
        }
        symlink(&tty_dir, self.sys.join("class/tty").join(name)).unwrap();
        fs::write(self.dev.join(name), b"").unwrap();
    }

    fn alias(&self, kind: &str, link: &str, tty: &str) {
        symlink(
            format!("../../{tty}"),
            self.dev.join("serial").join(kind).join(link),
        )
        .unwrap();
    }

    fn usb_device(&self, rel: &str, attrs: &[(&str, &str)]) {
        let dir = self.attrs(rel, attrs);
        self.link(&dir.join("subsystem"), "bus/usb");
    }

    fn usb_interface(&self, rel: &str, number: &str, driver: &str) {
        let dir = self.attrs(
            rel,
            &[
                ("bInterfaceNumber", number),
                (
                    "modalias",
                    "usb:v0000p0000d0000dc00dsc00dp00icFFiscFFipFFin00",
                ),
            ],
        );
        self.link(&dir.join("subsystem"), "bus/usb");
        self.link(&dir.join("driver"), &format!("bus/usb/drivers/{driver}"));
    }

    fn usb_serial_port(&self, iface_rel: &str, name: &str, driver: &str) {
        let port_rel = format!("{iface_rel}/{name}");
        let dir = self.dir(&port_rel);
        self.link(&dir.join("subsystem"), "bus/usb-serial");
        self.link(
            &dir.join("driver"),
            &format!("bus/usb-serial/drivers/{driver}"),
        );
        self.tty(
            name,
            &format!("{port_rel}/tty/{name}"),
            Some(&port_rel),
            &[("dev", "188:0")],
        );
    }
}

const HCD: &str = "devices/pci0000:00/0000:00:14.0";

fn build() -> Fixture {
    let f = Fixture::new();
    let hcd = f.dir(HCD);
    f.link(&hcd.join("subsystem"), "bus/pci");
    f.usb_device(
        &format!("{HCD}/usb1"),
        &[
            ("idVendor", "1d6b"),
            ("idProduct", "0002"),
            ("devpath", "0"),
        ],
    );

    // FTDI FT232R on port 4.
    f.usb_device(
        &format!("{HCD}/usb1/1-4"),
        &[
            ("idVendor", "0403"),
            ("idProduct", "6001"),
            ("serial", "A10XYZ12"),
            ("manufacturer", "FTDI"),
            ("product", "FT232R USB UART"),
            ("bcdDevice", "0600"),
            ("busnum", "1"),
            ("devpath", "4"),
            ("speed", "12"),
        ],
    );
    f.usb_interface(&format!("{HCD}/usb1/1-4/1-4:1.0"), "00", "ftdi_sio");
    f.usb_serial_port(&format!("{HCD}/usb1/1-4/1-4:1.0"), "ttyUSB0", "ftdi_sio");
    f.alias(
        "by-id",
        "usb-FTDI_FT232R_USB_UART_A10XYZ12-if00-port0",
        "ttyUSB0",
    );
    f.alias("by-path", "pci-0000:00:14.0-usb-0:4:1.0-port0", "ttyUSB0");

    // CP2105 dual UART on port 2.
    f.usb_device(
        &format!("{HCD}/usb1/1-2"),
        &[
            ("idVendor", "10c4"),
            ("idProduct", "ea70"),
            ("serial", "01A2B3C4"),
            ("manufacturer", "Silicon Labs"),
            ("product", "CP2105 Dual USB to UART Bridge Controller"),
            ("busnum", "1"),
            ("devpath", "2"),
        ],
    );
    f.usb_interface(&format!("{HCD}/usb1/1-2/1-2:1.0"), "00", "cp210x");
    f.usb_serial_port(&format!("{HCD}/usb1/1-2/1-2:1.0"), "ttyUSB1", "cp210x");
    f.usb_interface(&format!("{HCD}/usb1/1-2/1-2:1.1"), "01", "cp210x");
    f.usb_serial_port(&format!("{HCD}/usb1/1-2/1-2:1.1"), "ttyUSB2", "cp210x");

    // Arduino (CDC-ACM) behind a hub on port 3.
    f.usb_device(
        &format!("{HCD}/usb1/1-3"),
        &[
            ("idVendor", "05e3"),
            ("idProduct", "0608"),
            ("devpath", "3"),
        ],
    );
    f.usb_device(
        &format!("{HCD}/usb1/1-3/1-3.2"),
        &[
            ("idVendor", "2341"),
            ("idProduct", "0043"),
            ("serial", "85736323838351F0B180"),
            ("manufacturer", "Arduino (www.arduino.cc)"),
            ("busnum", "1"),
            ("devpath", "3.2"),
        ],
    );
    f.usb_interface(&format!("{HCD}/usb1/1-3/1-3.2/1-3.2:1.0"), "00", "cdc_acm");
    f.tty(
        "ttyACM0",
        &format!("{HCD}/usb1/1-3/1-3.2/1-3.2:1.0/tty/ttyACM0"),
        Some(&format!("{HCD}/usb1/1-3/1-3.2/1-3.2:1.0")),
        &[("dev", "166:0")],
    );

    // On-board 16550A (kernel ≥ 6.5 serial-base layout).
    let pnp = f.attrs("devices/pnp0/00:00", &[("id", "PNP0501")]);
    f.link(&pnp.join("subsystem"), "bus/pnp");
    f.link(&pnp.join("driver"), "bus/pnp/drivers/serial");
    let ctrl = f.dir("devices/pnp0/00:00/00:00:0");
    f.link(&ctrl.join("subsystem"), "bus/serial-base");
    let port = f.dir("devices/pnp0/00:00/00:00:0/00:00:0.0");
    f.link(&port.join("subsystem"), "bus/serial-base");
    f.link(&port.join("driver"), "bus/serial-base/drivers/port");
    f.tty(
        "ttyS0",
        "devices/pnp0/00:00/00:00:0/00:00:0.0/tty/ttyS0",
        Some("devices/pnp0/00:00/00:00:0/00:00:0.0"),
        &[
            ("type", "4"),
            ("port", "0x3F8"),
            ("irq", "4"),
            ("dev", "4:64"),
        ],
    );

    // Placeholder 8250 port without hardware.
    let placeholder = f.dir("devices/platform/serial8250/serial8250:0/serial8250:0.1");
    f.link(&placeholder.join("subsystem"), "bus/serial-base");
    f.tty(
        "ttyS1",
        "devices/platform/serial8250/serial8250:0/serial8250:0.1/tty/ttyS1",
        Some("devices/platform/serial8250/serial8250:0/serial8250:0.1"),
        &[("type", "0"), ("port", "0x2F8")],
    );

    // PCI serial card.
    let card = f.attrs(
        "devices/pci0000:00/0000:00:1c.0/0000:02:00.0",
        &[("vendor", "0x1415"), ("device", "0xc158")],
    );
    f.link(&card.join("subsystem"), "bus/pci");
    f.link(&card.join("driver"), "bus/pci/drivers/serial");
    f.tty(
        "ttyS4",
        "devices/pci0000:00/0000:00:1c.0/0000:02:00.0/tty/ttyS4",
        Some("devices/pci0000:00/0000:00:1c.0/0000:02:00.0"),
        &[("type", "10"), ("dev", "4:68")],
    );

    // Virtual ttys.
    f.tty(
        "rfcomm0",
        "devices/virtual/tty/rfcomm0",
        None,
        &[
            ("address", "00:11:22:33:44:55"),
            ("channel", "1"),
            ("dev", "216:0"),
        ],
    );
    f.tty(
        "tnt0",
        "devices/virtual/tty/tnt0",
        None,
        &[("dev", "240:0")],
    );
    f.tty("tty0", "devices/virtual/tty/tty0", None, &[("dev", "4:0")]);
    f.tty(
        "console",
        "devices/virtual/tty/console",
        None,
        &[("dev", "5:1")],
    );
    f
}

fn find<'a>(ports: &'a [DiscoveredPort], name: &str) -> &'a DiscoveredPort {
    ports
        .iter()
        .find(|p| p.port_name == name)
        .unwrap_or_else(|| panic!("{name} not found"))
}

#[test]
fn enumerates_expected_ports() {
    let f = build();
    let scan = discover_at(&f.sys, &f.dev);
    let names: Vec<_> = scan.ports.iter().map(|p| p.port_name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "/dev/rfcomm0",
            "/dev/tnt0",
            "/dev/ttyACM0",
            "/dev/ttyS0",
            "/dev/ttyS4",
            "/dev/ttyUSB0",
            "/dev/ttyUSB1",
            "/dev/ttyUSB2"
        ]
    );
    assert_eq!(scan.platform, "linux");
    assert!(scan.warnings.is_empty());
}

#[test]
fn ftdi_details() {
    let f = build();
    let scan = discover_at(&f.sys, &f.dev);
    let p = find(&scan.ports, "/dev/ttyUSB0");
    assert_eq!(p.transport, Transport::Usb);
    let usb = p.usb.as_ref().unwrap();
    assert_eq!((usb.vid, usb.pid), (0x0403, 0x6001));
    assert_eq!(usb.serial_number.as_deref(), Some("A10XYZ12"));
    assert_eq!(usb.interface_number, Some(0));
    assert_eq!(usb.product.as_deref(), Some("FT232R USB UART"));
    assert_eq!(usb.revision, Some(0x0600));
    assert_eq!(usb.location.as_deref(), Some("pci-0000:00:14.0-usb-0:4"));
    assert_eq!(usb.location_label.as_deref(), Some("USB bus 1, port 4"));
    assert_eq!(p.system.driver.as_deref(), Some("ftdi_sio"));
    assert_eq!(p.system.kernel_name.as_deref(), Some("188:0"));
    assert_eq!(p.manufacturer.as_deref(), Some("FTDI"));
    assert_eq!(p.device_label(), "FT232R USB UART");
    assert!(
        p.aliases
            .iter()
            .any(|a| a.ends_with("usb-FTDI_FT232R_USB_UART_A10XYZ12-if00-port0"))
    );
    assert_eq!(p.system.location_paths.len(), 1);
    assert!(p.system.access.as_ref().is_some_and(|a| a.readable));
    assert!(
        p.instance_identity.is_none(),
        "USB identity comes from serial/location"
    );

    let keys = derive_keys(p);
    assert_eq!(keys[0].value, "0403:6001:A10XYZ12:if0");
    assert_eq!(keys[1].kind, KeyKind::UsbPath);
}

#[test]
fn cp2105_ports_share_serial_but_not_interface() {
    let f = build();
    let scan = discover_at(&f.sys, &f.dev);
    let a = find(&scan.ports, "/dev/ttyUSB1");
    let b = find(&scan.ports, "/dev/ttyUSB2");
    assert_eq!(a.usb.as_ref().unwrap().interface_number, Some(0));
    assert_eq!(b.usb.as_ref().unwrap().interface_number, Some(1));
    assert_eq!(a.system.driver.as_deref(), Some("cp210x"));
    assert_ne!(derive_keys(a)[0].value, derive_keys(b)[0].value);
    assert_eq!(derive_keys(b)[0].value, "10C4:EA70:01A2B3C4:if1");
}

#[test]
fn cdc_acm_behind_hub() {
    let f = build();
    let scan = discover_at(&f.sys, &f.dev);
    let p = find(&scan.ports, "/dev/ttyACM0");
    let usb = p.usb.as_ref().unwrap();
    assert_eq!((usb.vid, usb.pid), (0x2341, 0x0043));
    assert_eq!(usb.location.as_deref(), Some("pci-0000:00:14.0-usb-0:3.2"));
    assert_eq!(usb.device_node.as_deref(), Some("1-3.2"));
    assert_eq!(p.system.driver.as_deref(), Some("cdc_acm"));
    assert!(p.extra.iter().any(|e| e.value == "USB CDC-ACM"));
}

#[test]
fn builtin_and_pci_uarts() {
    let f = build();
    let scan = discover_at(&f.sys, &f.dev);
    let s0 = find(&scan.ports, "/dev/ttyS0");
    assert_eq!(s0.transport, Transport::Builtin);
    assert_eq!(
        s0.description.as_deref(),
        Some("Built-in serial port (16550A UART)")
    );
    assert_eq!(
        s0.system.hardware_ids.first().map(String::as_str),
        Some("PNP0501")
    );
    assert_eq!(s0.system.driver.as_deref(), Some("serial"));
    assert!(
        s0.extra
            .iter()
            .any(|e| e.name == "Legacy PC name" && e.value == "COM1")
    );
    let identity = s0.instance_identity.as_ref().unwrap();
    assert_eq!(identity.quality, InstanceQuality::Stable);
    assert_eq!(identity.value, "linux:/sys/devices/pnp0/00:00#ttyS0");

    let s4 = find(&scan.ports, "/dev/ttyS4");
    assert_eq!(s4.transport, Transport::Pci);
    assert_eq!(s4.manufacturer.as_deref(), Some("Oxford Semiconductor"));
    assert_eq!(
        s4.description.as_deref(),
        Some("PCI serial port (1415:C158)")
    );
}

#[test]
fn virtual_ttys() {
    let f = build();
    let scan = discover_at(&f.sys, &f.dev);
    let bt = find(&scan.ports, "/dev/rfcomm0");
    assert_eq!(bt.transport, Transport::Bluetooth);
    let info = bt.bluetooth.as_ref().unwrap();
    assert_eq!(info.address.as_deref(), Some("00:11:22:33:44:55"));
    assert_eq!(info.channel, Some(1));
    assert_eq!(info.service_key, None);
    assert_eq!(derive_keys(bt)[0].value, "00:11:22:33:44:55");

    let tnt = find(&scan.ports, "/dev/tnt0");
    assert_eq!(tnt.transport, Transport::Virtual);
    let v = tnt.virtual_port.as_ref().unwrap();
    assert_eq!(v.provider, "tty0tty");
    assert_eq!(v.detail.as_deref(), Some("Paired with /dev/tnt1"));
}

#[test]
fn real_system_scan_does_not_fail() {
    let scan = super::discover();
    assert_eq!(scan.platform, "linux");
    for port in &scan.ports {
        assert!(port.port_name.starts_with("/dev/"));
    }
}
