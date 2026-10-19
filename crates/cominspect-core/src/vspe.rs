//! Eterlogic VSPE (Virtual Serial Ports Emulator) configuration files.
//!
//! VSPE creates virtual serial ports and connects ports to each other: a
//! Splitter shares one port (usually a real one) through several virtual
//! ports, a Pair joins two virtual ports, a TCP server makes a port reachable
//! over the network, and so on. ComInspect reads VSPE's configuration file
//! (never VSPE itself) to show those links.
//!
//! A `.vspe` file starts with a small header (a 32-bit little-endian
//! `0x11223344` at offset 4), followed by one record per device: the device
//! type (`Splitter`) and its settings string, each stored as a 32-bit
//! little-endian length and the bytes. VSPE 1.5.1 and later store more
//! fields around them, so records are found by their shape rather than at
//! fixed offsets. Among them is the name the user gave the device, right
//! after the settings: a 32-bit zero, then the name as a 32-bit character
//! count and UTF-16LE text.
//!
//! The settings strings are documented by Eterlogic for the VSPE API
//! ("Devices initialization"), for example `10;9;0;19200,0,8,1,0,0;0;0;0` for
//! a Splitter that shares COM9 as COM10. VSPE 1.5 added Splitters with up to
//! eight virtual ports, stored as
//! `v2;<source>;<options>;<serial settings>;<count>;<port>;…` where every
//! virtual port has the same number of fields and starts with its number.
//! Later versions write `v3` with more fields per port in the same layout.
//! Settings in other versioned formats are kept but not interpreted.

use serde::{Deserialize, Serialize};

const MAGIC: u32 = 0x1122_3344;
/// Longest device type name and settings string accepted in a record.
const MAX_KIND_LEN: usize = 32;
const MAX_SETTINGS_LEN: usize = 8192;
/// A Splitter has at most this many virtual ports.
const MAX_SPLITTER_PORTS: usize = 8;
/// Longest device name accepted, in UTF-16 units.
const MAX_LABEL_LEN: usize = 256;

/// One VSPE device from a configuration file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VspeDevice {
    /// Device type as VSPE names it: `Splitter`, `Pair`, `Connector` …
    pub kind: String,
    /// The settings string as stored in the file.
    pub settings: String,
    /// The name the user gave the device in VSPE, if any.
    #[serde(default)]
    pub label: Option<String>,
    /// The ports involved, when ComInspect understands the settings.
    pub layout: Option<VspeLayout>,
}

/// How a VSPE device connects ports. Port names are `COM<n>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VspeLayout {
    /// Shares `source` through the virtual `ports` VSPE creates.
    Splitter {
        source: String,
        ports: Vec<String>,
        baud: Option<u32>,
    },
    /// A virtual port that two programs open to talk to each other.
    Connector { port: String },
    /// Two virtual ports connected to each other.
    Pair { ports: Vec<String> },
    /// Copies data between two existing ports.
    Redirector { ports: Vec<String> },
    /// Makes `port` reachable over the network.
    Network {
        port: String,
        /// "TCP server", "TCP client" or "UDP".
        protocol: String,
        /// "port 5555" or "192.168.10.2:5555".
        address: String,
    },
}

impl VspeLayout {
    /// Every port the device uses or creates.
    pub fn ports(&self) -> Vec<&str> {
        match self {
            VspeLayout::Splitter { source, ports, .. } => std::iter::once(source.as_str())
                .chain(ports.iter().map(String::as_str))
                .collect(),
            VspeLayout::Connector { port } | VspeLayout::Network { port, .. } => vec![port],
            VspeLayout::Pair { ports } | VspeLayout::Redirector { ports } => {
                ports.iter().map(String::as_str).collect()
            }
        }
    }
}

impl VspeDevice {
    /// One line for people: "Splitter: COM5 shared as COM8 and COM10 (9600
    /// baud)", or `Splitter "D750 BT Port": …` when the device has a name.
    pub fn describe(&self) -> String {
        let name = |kind: &str| match &self.label {
            Some(label) => format!("{kind} \"{label}\""),
            None => kind.to_string(),
        };
        let Some(layout) = &self.layout else {
            return format!(
                "{}: settings not recognized ({})",
                name(&self.kind),
                self.settings
            );
        };
        match layout {
            VspeLayout::Splitter {
                source,
                ports,
                baud,
            } => format!(
                "{}: {source} shared as {}{}",
                name("Splitter"),
                join(ports),
                baud.map(|b| format!(" ({b} baud)")).unwrap_or_default()
            ),
            VspeLayout::Connector { port } => format!("{}: {port}", name("Connector")),
            VspeLayout::Pair { ports } => format!("{}: {}", name("Pair"), ports.join(" ↔ ")),
            VspeLayout::Redirector { ports } => {
                format!("{}: {}", name("Redirector"), ports.join(" ↔ "))
            }
            VspeLayout::Network {
                port,
                protocol,
                address,
            } => format!("{}: {port} on {address}", name(protocol)),
        }
    }
}

/// "COM8", "COM8 and COM10", "COM8, COM10 and COM11".
fn join(ports: &[String]) -> String {
    match ports {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VspeError {
    #[error("this is not a VSPE configuration file")]
    NotVspe,
}

/// Reads the devices from the contents of a `.vspe` file.
pub fn parse_config(bytes: &[u8]) -> Result<Vec<VspeDevice>, VspeError> {
    if u32_at(bytes, 4) != Some(MAGIC) {
        return Err(VspeError::NotVspe);
    }
    let mut devices = Vec::new();
    let mut at = 8;
    while at < bytes.len() {
        match record_at(bytes, at) {
            Some((device, next)) => {
                devices.push(device);
                at = next;
            }
            None => at += 1,
        }
    }
    Ok(devices)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(raw.try_into().ok()?))
}

/// A length-prefixed string at `at`: its bytes and where the next field
/// starts.
fn field_at(bytes: &[u8], at: usize, max: usize) -> Option<(&[u8], usize)> {
    let len = u32_at(bytes, at)? as usize;
    if len > max {
        return None;
    }
    let start = at + 4;
    let end = start.checked_add(len)?;
    Some((bytes.get(start..end)?, end))
}

/// A device record at `at`: a known device type followed by printable
/// settings and, in newer files, the device's name.
fn record_at(bytes: &[u8], at: usize) -> Option<(VspeDevice, usize)> {
    let (kind, next) = field_at(bytes, at, MAX_KIND_LEN)?;
    if kind.is_empty() || !kind.iter().all(u8::is_ascii_alphanumeric) {
        return None;
    }
    let kind = std::str::from_utf8(kind).ok()?;
    known_kind(kind)?;
    let (settings, next) = field_at(bytes, next, MAX_SETTINGS_LEN)?;
    if settings.iter().any(u8::is_ascii_control) {
        return None;
    }
    let settings = String::from_utf8_lossy(settings).into_owned();
    let (label, next) = match label_at(bytes, next) {
        Some((label, end)) => (Some(label), end),
        None => (None, next),
    };
    Some((
        VspeDevice {
            layout: layout(kind, &settings),
            kind: kind.to_string(),
            settings,
            label,
        },
        next,
    ))
}

/// The device name after the settings: a 32-bit zero, then a 32-bit
/// character count and that many UTF-16LE characters of printable text.
fn label_at(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    if u32_at(bytes, at)? != 0 {
        return None;
    }
    let count = u32_at(bytes, at + 4)? as usize;
    if !(1..=MAX_LABEL_LEN).contains(&count) {
        return None;
    }
    let start = at + 8;
    let raw = bytes.get(start..start + 2 * count)?;
    let units = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|u| u16::from_le_bytes(*u));
    let label = char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()?;
    if label.chars().any(char::is_control) {
        return None;
    }
    let label = label.trim();
    (!label.is_empty()).then(|| (label.to_string(), start + 2 * count))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Splitter,
    Connector,
    Pair,
    Redirector,
    TcpServer,
    TcpClient,
    Udp,
    /// Known VSPE devices whose settings are not interpreted (Bridge, Serial
    /// Router).
    Other,
}

fn known_kind(name: &str) -> Option<Kind> {
    Some(match name.to_ascii_lowercase().as_str() {
        "splitter" => Kind::Splitter,
        "connector" => Kind::Connector,
        "pair" => Kind::Pair,
        "redirector" | "serialredirector" => Kind::Redirector,
        "tcpserver" => Kind::TcpServer,
        "tcpclient" => Kind::TcpClient,
        "udpmanager" | "udp" => Kind::Udp,
        "bridge" | "router" | "serialrouter" => Kind::Other,
        _ => return None,
    })
}

/// `COM<n>` from a VSPE port number.
fn com_port(field: &str) -> Option<String> {
    let n: u32 = field.trim().parse().ok()?;
    (1..=4096).contains(&n).then(|| format!("COM{n}"))
}

/// The baud rate in serial settings: `19200,0,8,1,0,0` (documented) or
/// `v003,9600,8,…` (VSPE 1.5 and later).
fn baud(settings: &str) -> Option<u32> {
    let mut parts = settings.split(',');
    let first = parts.next()?;
    let value = if first.starts_with('v') {
        parts.next()?
    } else {
        first
    };
    value
        .trim()
        .parse()
        .ok()
        .filter(|b| (50..=10_000_000).contains(b))
}

/// `v2`, `v3` …: a settings format newer than the documented one.
fn versioned(fields: &[&str]) -> bool {
    fields.first().is_some_and(|f| {
        f.len() >= 2 && f.starts_with('v') && f[1..].bytes().all(|b| b.is_ascii_digit())
    })
}

fn layout(kind: &str, settings: &str) -> Option<VspeLayout> {
    let fields: Vec<&str> = settings.split(';').collect();
    let port = |i: usize| fields.get(i).and_then(|f| com_port(f));
    let kind = known_kind(kind)?;
    if kind == Kind::Splitter {
        return splitter(&fields);
    }
    if versioned(&fields) {
        return None;
    }
    let network = |protocol: &str, port: String, address: String| VspeLayout::Network {
        port,
        protocol: protocol.into(),
        address,
    };
    match kind {
        // ComPortIndex;EmulateBaudRate
        Kind::Connector => Some(VspeLayout::Connector { port: port(0)? }),
        // ComPortIndex1;ComPortIndex2;EmulateBaudRate
        Kind::Pair => Some(VspeLayout::Pair {
            ports: vec![port(0)?, port(1)?],
        }),
        // ComPortIndex1;SerialSettings1;ComPortIndex2;SerialSettings2;UseModemRegisters
        Kind::Redirector => Some(VspeLayout::Redirector {
            ports: vec![port(0)?, port(2)?],
        }),
        // TcpPort;ComPortIndex;SerialSettings;UseDtrRts;InterfaceIP
        Kind::TcpServer => {
            let tcp: u16 = fields.first()?.trim().parse().ok()?;
            Some(network("TCP server", port(1)?, format!("port {tcp}")))
        }
        // TcpHost;TcpPort;ComPortIndex;SerialSettings;…
        Kind::TcpClient => {
            let tcp: u16 = fields.get(1)?.trim().parse().ok()?;
            let host = fields.first()?.trim();
            (!host.is_empty()).then_some(())?;
            Some(network("TCP client", port(2)?, format!("{host}:{tcp}")))
        }
        // Host;Port;ComPortIndex;SerialSettings
        Kind::Udp => {
            let udp: u16 = fields.get(1)?.trim().parse().ok()?;
            let host = fields.first()?.trim();
            (!host.is_empty()).then_some(())?;
            Some(network("UDP", port(2)?, format!("{host}:{udp}")))
        }
        Kind::Splitter | Kind::Other => None,
    }
}

fn splitter(fields: &[&str]) -> Option<VspeLayout> {
    if !versioned(fields) {
        // Documented: VirtualComPortIndex;RealComPortIndex;ReadOnly;SerialSettings;…
        return Some(VspeLayout::Splitter {
            source: com_port(fields.get(1)?)?,
            ports: vec![com_port(fields.first()?)?],
            baud: fields.get(3).and_then(|s| baud(s)),
        });
    }
    // VSPE 1.5 and later: v2;<source>;<options>;<serial settings>;<count>;<port>;…
    // with five fields per port, and v3 with nine.
    let source = com_port(fields.get(1)?)?;
    let baud = fields.get(3).and_then(|s| baud(s));
    let count: usize = fields.get(4)?.trim().parse().ok()?;
    if !(1..=MAX_SPLITTER_PORTS).contains(&count) {
        return None;
    }
    let rest = fields.get(5..)?;
    if rest.len() < count || rest.len() % count != 0 {
        return None;
    }
    let per_port = rest.len() / count;
    let ports = rest
        .chunks(per_port)
        .map(|port| com_port(port[0]))
        .collect::<Option<Vec<_>>>()?;
    // Every virtual port is a different port, and none is the shared one.
    for (i, port) in ports.iter().enumerate() {
        if *port == source || ports[..i].contains(port) {
            return None;
        }
    }
    Some(VspeLayout::Splitter {
        source,
        ports,
        baud,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configuration saved by VSPE 1.5 on a laptop: four Splitters.
    const SPLITTERS: &[u8] = include_bytes!("../tests/data/splitters.vspe");
    /// The startup configuration of the same laptop saved by a later VSPE:
    /// `v3` Splitters, one of them with a name.
    const SPLITTERS_V3: &[u8] = include_bytes!("../tests/data/splitters-v3.vspe");

    fn splitter(source: &str, ports: &[&str], baud: u32) -> Option<VspeLayout> {
        Some(VspeLayout::Splitter {
            source: source.into(),
            ports: ports.iter().map(|p| p.to_string()).collect(),
            baud: Some(baud),
        })
    }

    #[test]
    fn reads_multi_port_splitters_saved_by_vspe() {
        let devices = parse_config(SPLITTERS).unwrap();
        let layouts: Vec<_> = devices.iter().map(|d| d.layout.clone()).collect();
        assert_eq!(
            layouts,
            [
                splitter("COM5", &["COM8", "COM10"], 9600),
                splitter("COM4", &["COM20", "COM21", "COM22"], 9600),
                splitter("COM13", &["COM1", "COM2"], 9600),
                splitter("COM14", &["COM23", "COM24", "COM25"], 4800),
            ]
        );
        assert!(devices.iter().all(|d| d.kind == "Splitter"));
        assert!(devices.iter().all(|d| d.label.is_none()));
        assert_eq!(
            devices[1].describe(),
            "Splitter: COM4 shared as COM20, COM21 and COM22 (9600 baud)"
        );
    }

    #[test]
    fn reads_v3_splitters_and_their_names() {
        let devices = parse_config(SPLITTERS_V3).unwrap();
        let layouts: Vec<_> = devices.iter().map(|d| d.layout.clone()).collect();
        assert_eq!(
            layouts,
            [
                splitter("COM5", &["COM8", "COM10"], 9600),
                splitter("COM4", &["COM20", "COM21", "COM22"], 9600),
                splitter("COM13", &["COM1", "COM2"], 9600),
                splitter("COM14", &["COM23", "COM24", "COM25"], 4800),
                splitter("COM15", &["COM26", "COM28"], 9600),
            ]
        );
        let labels: Vec<_> = devices.iter().map(|d| d.label.as_deref()).collect();
        assert_eq!(labels, [None, None, None, None, Some("D750 BT Port")]);
        assert_eq!(
            devices[4].describe(),
            "Splitter \"D750 BT Port\": COM15 shared as COM26 and COM28 (9600 baud)"
        );
    }

    /// The examples from Eterlogic's "Devices initialization" page.
    #[test]
    fn understands_the_documented_settings() {
        let cases = [
            (
                "Connector",
                "9;0",
                VspeLayout::Connector {
                    port: "COM9".into(),
                },
            ),
            (
                "Splitter",
                "10;9;0;19200,0,8,1,0,0;0;0;0",
                splitter("COM9", &["COM10"], 19200).unwrap(),
            ),
            (
                "Pair",
                "21;22;0",
                VspeLayout::Pair {
                    ports: vec!["COM21".into(), "COM22".into()],
                },
            ),
            (
                "TcpServer",
                "5555;1;19200,0,8,1,0,0;1;127.0.0.1;0;0",
                VspeLayout::Network {
                    port: "COM1".into(),
                    protocol: "TCP server".into(),
                    address: "port 5555".into(),
                },
            ),
            (
                "TcpClient",
                "192.168.10.2;5555;1;19200,0,8,1,0,0;0;30;MyScript;0;0",
                VspeLayout::Network {
                    port: "COM1".into(),
                    protocol: "TCP client".into(),
                    address: "192.168.10.2:5555".into(),
                },
            ),
            (
                "UdpManager",
                "192.168.10.2;5555;1;19200,0,8,1,0,0",
                VspeLayout::Network {
                    port: "COM1".into(),
                    protocol: "UDP".into(),
                    address: "192.168.10.2:5555".into(),
                },
            ),
            (
                "SerialRedirector",
                "1;19200,0,8,1,0,0;2;19200,0,8,1,0,0;0",
                VspeLayout::Redirector {
                    ports: vec!["COM1".into(), "COM2".into()],
                },
            ),
        ];
        for (kind, settings, expected) in cases {
            assert_eq!(layout(kind, settings), Some(expected), "{kind} {settings}");
        }
    }

    #[test]
    fn leaves_unknown_formats_alone() {
        assert_eq!(layout("Connector", "v2;9;0"), None);
        assert_eq!(layout("Bridge", "1;2"), None);
        // Splitters whose port list does not add up.
        for settings in [
            "v2;5;28;v003,9600;3;8;6;0;;",
            "v2;5;28;v003,9600;2;8;x;0;;;y;6;0;;",
            "v3;5;28;v004,9600;0;",
            "v3;5;28;v004,9600;9;8;9;10;11;12;13;14;15;16",
            "v3;5;28;v004,9600;2;8;6;0;0;0;0;0;0;0;8;6;0;0;0;0;0;0;0",
            "v3;5;28;v004,9600;2;8;6;0;0;0;0;0;0;0;5;6;0;0;0;0;0;0;0",
            "v3;x;28;v004,9600;1;8;6;0;0;0;0;0;0;0",
        ] {
            assert_eq!(layout("Splitter", settings), None, "{settings}");
        }
        assert_eq!(
            layout(
                "Splitter",
                "v3;15;28;v004,9600,8,0;2;26;6;0;0;0;0;0;0;0;28;6;0;0;0;0;0;0;0"
            ),
            splitter("COM15", &["COM26", "COM28"], 9600)
        );
    }

    fn label(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(text.encode_utf16().count() as u32).to_le_bytes());
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
    }

    #[test]
    fn reads_device_names() {
        let mut file = vec![0, 0, 0, 0];
        file.extend_from_slice(&MAGIC.to_le_bytes());
        file.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0]);
        record(&mut file, "Pair");
        record(&mut file, "21;22;0");
        label(&mut file, "FT-991A ↔ N1MM");
        record(&mut file, "Connector");
        record(&mut file, "9;0");
        label(&mut file, "");
        record(&mut file, "Connector");
        record(&mut file, "10;0");
        // Not a name: control characters.
        label(&mut file, "\u{1}\u{2}");
        let devices = parse_config(&file).unwrap();
        let labels: Vec<_> = devices.iter().map(|d| d.label.as_deref()).collect();
        assert_eq!(labels, [Some("FT-991A ↔ N1MM"), None, None]);
        assert_eq!(
            devices[0].describe(),
            "Pair \"FT-991A ↔ N1MM\": COM21 ↔ COM22"
        );
    }

    fn record(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }

    /// Titles, descriptions and state fields around the records (VSPE 1.5.1
    /// and later) do not hide devices or pass for devices.
    #[test]
    fn finds_records_between_other_fields() {
        let mut file = vec![0, 0, 0, 0];
        file.extend_from_slice(&MAGIC.to_le_bytes());
        file.extend_from_slice(&1u32.to_le_bytes());
        record(&mut file, "Shack");
        record(&mut file, "Radios; GPS and CAT");
        file.extend_from_slice(&2u32.to_le_bytes());
        record(&mut file, "Pair");
        record(&mut file, "21;22;0");
        record(&mut file, "Connector");
        record(&mut file, "for fldigi");
        file.extend_from_slice(&1u32.to_le_bytes());
        record(&mut file, "Connector");
        record(&mut file, "9;0");
        file.extend_from_slice(&[0; 12]);
        let devices = parse_config(&file).unwrap();
        let kinds: Vec<_> = devices
            .iter()
            .map(|d| (d.kind.as_str(), d.settings.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("Pair", "21;22;0"),
                ("Connector", "for fldigi"),
                ("Connector", "9;0")
            ]
        );
        // A title that happens to be a device name is harmless: its
        // "settings" are not understood, so it links nothing.
        assert_eq!(devices[1].layout, None);
    }

    #[test]
    fn rejects_other_files_and_survives_damage() {
        assert_eq!(parse_config(b""), Err(VspeError::NotVspe));
        assert_eq!(
            parse_config(b"<?xml version=\"1.0\"?>"),
            Err(VspeError::NotVspe)
        );
        for file in [SPLITTERS, SPLITTERS_V3] {
            for cut in 0..file.len() {
                let _ = parse_config(&file[..cut]);
            }
            let mut noisy = file.to_vec();
            for (i, b) in noisy.iter_mut().enumerate().skip(8) {
                if i % 7 == 0 {
                    *b = b.wrapping_mul(31).wrapping_add(7);
                }
            }
            let _ = parse_config(&noisy);
        }
        let mut huge = SPLITTERS[..8].to_vec();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_config(&huge), Ok(vec![]));
    }
}
