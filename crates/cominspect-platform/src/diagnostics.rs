//! Explicit, user-initiated serial diagnostics.
//!
//! Nothing in this module runs automatically. Opening a serial port can
//! change the DTR and RTS lines, which many stations use to key PTT or CW, so:
//!
//! * DTR and RTS are driven **low** immediately after opening unless the user
//!   explicitly asks otherwise (on Linux the kernel still asserts both for a
//!   moment when any program opens a port; this cannot be prevented);
//! * CAT probes only send read-only commands chosen by the user;
//! * the PTT test is time-limited and always releases the line.

use std::io::{Read, Write};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

use crate::cat::{self, CatProtocol};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParitySetting {
    None,
    Even,
    Odd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowSetting {
    None,
    Hardware,
    Software,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialSettings {
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: ParitySetting,
    pub stop_bits: u8,
    pub flow_control: FlowSetting,
}

impl Default for SerialSettings {
    fn default() -> Self {
        SerialSettings {
            baud_rate: 9600,
            data_bits: 8,
            parity: ParitySetting::None,
            stop_bits: 1,
            flow_control: FlowSetting::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModemLines {
    pub cts: Option<bool>,
    pub dsr: Option<bool>,
    pub dcd: Option<bool>,
    pub ri: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenOutcome {
    Opened,
    InUse,
    PermissionDenied,
    NotFound,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessUsage {
    pub pid: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTestReport {
    pub port: String,
    pub outcome: OpenOutcome,
    pub message: String,
    pub lines: Option<ModemLines>,
    /// Processes known to hold the port open (Linux, same user only).
    pub users: Vec<ProcessUsage>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatQueryReport {
    pub port: String,
    pub outcome: OpenOutcome,
    pub sent_hex: String,
    pub received_hex: String,
    pub received_text: String,
    pub recognized: bool,
    pub summary: String,
    pub elapsed_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlLine {
    Rts,
    Dtr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PttTestReport {
    pub port: String,
    pub outcome: OpenOutcome,
    pub line: ControlLine,
    pub held_ms: u64,
    pub message: String,
}

/// Longest PTT test allowed.
pub const MAX_PTT_MS: u64 = 3000;

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn printable(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '·'
            }
        })
        .collect()
}

fn classify_error(error: &serialport::Error) -> (OpenOutcome, String) {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();
    let outcome = match error.kind() {
        serialport::ErrorKind::NoDevice => OpenOutcome::NotFound,
        serialport::ErrorKind::Io(std::io::ErrorKind::NotFound) => OpenOutcome::NotFound,
        serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) => {
            // Windows reports "Access is denied" when another program has
            // the port open.
            if cfg!(windows) {
                OpenOutcome::InUse
            } else {
                OpenOutcome::PermissionDenied
            }
        }
        _ if lower.contains("busy") || lower.contains("in use") => OpenOutcome::InUse,
        _ if lower.contains("denied") => {
            if cfg!(windows) {
                OpenOutcome::InUse
            } else {
                OpenOutcome::PermissionDenied
            }
        }
        _ => OpenOutcome::Error,
    };
    let message = match outcome {
        OpenOutcome::InUse => format!("The port is in use by another program ({text})."),
        OpenOutcome::PermissionDenied => format!("Permission denied ({text})."),
        OpenOutcome::NotFound => {
            format!("The port does not exist or the device is unplugged ({text}).")
        }
        _ => format!("The port could not be opened ({text})."),
    };
    (outcome, message)
}

fn open_port(
    port: &str,
    settings: &SerialSettings,
    timeout: Duration,
) -> Result<Box<dyn SerialPort>, serialport::Error> {
    let builder = serialport::new(port, settings.baud_rate.clamp(50, 4_000_000))
        .data_bits(if settings.data_bits == 7 {
            DataBits::Seven
        } else {
            DataBits::Eight
        })
        .parity(match settings.parity {
            ParitySetting::None => Parity::None,
            ParitySetting::Even => Parity::Even,
            ParitySetting::Odd => Parity::Odd,
        })
        .stop_bits(if settings.stop_bits == 2 {
            StopBits::Two
        } else {
            StopBits::One
        })
        .flow_control(match settings.flow_control {
            FlowSetting::None => FlowControl::None,
            FlowSetting::Hardware => FlowControl::Hardware,
            FlowSetting::Software => FlowControl::Software,
        })
        .timeout(timeout)
        .dtr_on_open(false);
    let mut handle = builder.open()?;
    if settings.flow_control != FlowSetting::Hardware {
        // Release RTS right away: the OS may have asserted it on open.
        let _ = handle.write_request_to_send(false);
    }
    Ok(handle)
}

fn read_lines(port: &mut dyn SerialPort) -> ModemLines {
    ModemLines {
        cts: port.read_clear_to_send().ok(),
        dsr: port.read_data_set_ready().ok(),
        dcd: port.read_carrier_detect().ok(),
        ri: port.read_ring_indicator().ok(),
    }
}

/// Refuses to touch a port another program is known to be using.
fn in_use_message(port: &str) -> Option<String> {
    let users = port_users(port);
    (!users.is_empty()).then(|| {
        let names: Vec<_> = users
            .iter()
            .map(|u| format!("{} (pid {})", u.name, u.pid))
            .collect();
        format!(
            "The port is open in {}. It was not touched.",
            names.join(", ")
        )
    })
}

/// Checks whether a port can be opened and reports the modem status lines.
/// On Linux, processes that already hold the port are detected *without*
/// opening it.
pub fn open_test(port: &str) -> OpenTestReport {
    let started = Instant::now();
    let users = port_users(port);
    if !users.is_empty() {
        let names: Vec<_> = users
            .iter()
            .map(|u| format!("{} (pid {})", u.name, u.pid))
            .collect();
        return OpenTestReport {
            port: port.to_string(),
            outcome: OpenOutcome::InUse,
            message: format!(
                "The port is open in {}. It was not touched.",
                names.join(", ")
            ),
            lines: None,
            users,
            elapsed_ms: started.elapsed().as_millis() as u64,
        };
    }
    match open_port(port, &SerialSettings::default(), Duration::from_millis(100)) {
        Ok(mut handle) => {
            let lines = read_lines(handle.as_mut());
            drop(handle);
            OpenTestReport {
                port: port.to_string(),
                outcome: OpenOutcome::Opened,
                message: "The port opened successfully and was closed again. No data was sent."
                    .into(),
                lines: Some(lines),
                users,
                elapsed_ms: started.elapsed().as_millis() as u64,
            }
        }
        Err(e) => {
            let (outcome, message) = classify_error(&e);
            OpenTestReport {
                port: port.to_string(),
                outcome,
                message,
                lines: None,
                users,
                elapsed_ms: started.elapsed().as_millis() as u64,
            }
        }
    }
}

/// Sends one read-only CAT probe and reports the raw reply.
pub fn cat_query(
    port: &str,
    settings: &SerialSettings,
    protocol: CatProtocol,
    civ_address: u8,
    timeout: Duration,
) -> CatQueryReport {
    let started = Instant::now();
    let sent = cat::request_bytes(protocol, civ_address);
    let report = |outcome, received: &[u8], recognized, summary: String| CatQueryReport {
        port: port.to_string(),
        outcome,
        sent_hex: hex(&sent),
        received_hex: hex(received),
        received_text: printable(received),
        recognized,
        summary,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };
    let timeout = timeout.clamp(Duration::from_millis(200), Duration::from_secs(5));
    if let Some(message) = in_use_message(port) {
        return report(OpenOutcome::InUse, &[], false, message);
    }
    let mut handle = match open_port(port, settings, Duration::from_millis(50)) {
        Ok(h) => h,
        Err(e) => {
            let (outcome, message) = classify_error(&e);
            return report(outcome, &[], false, message);
        }
    };
    let _ = handle.clear(serialport::ClearBuffer::All);
    if let Err(e) = handle.write_all(&sent).and_then(|_| handle.flush()) {
        return report(
            OpenOutcome::Error,
            &[],
            false,
            format!("Writing to the port failed: {e}"),
        );
    }
    let deadline = Instant::now() + timeout;
    let mut received = Vec::new();
    let mut buf = [0u8; 256];
    while Instant::now() < deadline && received.len() < 4096 {
        match handle.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                received.extend_from_slice(&buf[..n]);
                if cat::response_complete(protocol, &received, &sent) {
                    // Allow trailing bytes of the same reply to arrive.
                    thread::sleep(Duration::from_millis(30));
                    if let Ok(n) = handle.read(&mut buf) {
                        received.extend_from_slice(&buf[..n]);
                    }
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                return report(
                    OpenOutcome::Error,
                    &received,
                    false,
                    format!("Reading from the port failed: {e}"),
                );
            }
        }
    }
    drop(handle);
    let interpretation = cat::interpret_response(protocol, &received, &sent);
    report(
        OpenOutcome::Opened,
        &received,
        interpretation.recognized,
        interpretation.summary,
    )
}

/// Asserts RTS or DTR for `duration` and releases it again. This keys the
/// transmitter on most PTT interfaces — callers must obtain explicit
/// confirmation from the user first.
pub fn ptt_test(port: &str, line: ControlLine, duration: Duration) -> PttTestReport {
    let duration = duration.min(Duration::from_millis(MAX_PTT_MS));
    if let Some(message) = in_use_message(port) {
        return PttTestReport {
            port: port.to_string(),
            outcome: OpenOutcome::InUse,
            line,
            held_ms: 0,
            message,
        };
    }
    let mut handle = match open_port(port, &SerialSettings::default(), Duration::from_millis(50)) {
        Ok(h) => h,
        Err(e) => {
            let (outcome, message) = classify_error(&e);
            return PttTestReport {
                port: port.to_string(),
                outcome,
                line,
                held_ms: 0,
                message,
            };
        }
    };
    let set = |h: &mut Box<dyn SerialPort>, on: bool| match line {
        ControlLine::Rts => h.write_request_to_send(on),
        ControlLine::Dtr => h.write_data_terminal_ready(on),
    };
    let started = Instant::now();
    let result = set(&mut handle, true);
    if result.is_ok() {
        thread::sleep(duration);
    }
    // Always release, even if asserting failed.
    let release = set(&mut handle, false);
    let held_ms = started.elapsed().as_millis() as u64;
    drop(handle);
    let line_name = match line {
        ControlLine::Rts => "RTS",
        ControlLine::Dtr => "DTR",
    };
    match (result, release) {
        (Ok(()), Ok(())) => PttTestReport {
            port: port.to_string(),
            outcome: OpenOutcome::Opened,
            line,
            held_ms,
            message: format!(
                "{line_name} was asserted for {held_ms} ms and released. If the radio transmitted, this is its PTT port and line."
            ),
        },
        (Err(e), _) | (_, Err(e)) => PttTestReport {
            port: port.to_string(),
            outcome: OpenOutcome::Error,
            line,
            held_ms,
            message: format!("Changing {line_name} failed: {e}"),
        },
    }
}

/// Processes holding a device node open (Linux; only processes the current
/// user may inspect are visible).
#[cfg(target_os = "linux")]
pub fn port_users(port: &str) -> Vec<ProcessUsage> {
    use std::path::Path;
    let Ok(target) = std::fs::canonicalize(port) else {
        return Vec::new();
    };
    let mut users = Vec::new();
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return users;
    };
    let me = std::process::id();
    for entry in procs.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        let holds = fds
            .flatten()
            .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|t| t == target));
        if holds {
            let name =
                std::fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("comm"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| "unknown".into());
            users.push(ProcessUsage { pid, name });
        }
    }
    users
}

#[cfg(not(target_os = "linux"))]
pub fn port_users(_port: &str) -> Vec<ProcessUsage> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_helpers() {
        assert_eq!(hex(&[0xFE, 0x01]), "FE 01");
        assert_eq!(printable(b"ID;\r\n"), "ID;··");
    }

    #[test]
    fn missing_port_is_reported_not_panicking() {
        let report = open_test("/dev/does-not-exist-cominspect");
        assert_ne!(report.outcome, OpenOutcome::Opened);
        let ptt = ptt_test(
            "/dev/does-not-exist-cominspect",
            ControlLine::Rts,
            Duration::from_millis(10),
        );
        assert_eq!(ptt.held_ms, 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detects_processes_holding_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake-tty");
        std::fs::write(&path, b"").unwrap();
        // Our own process is excluded, so a child holds the file open.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("exec 3<'{}'; sleep 5", path.display()))
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut users = Vec::new();
        while users.is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            users = port_users(path.to_str().unwrap());
        }
        let _ = child.kill();
        let _ = child.wait();
        assert!(users.iter().any(|u| u.pid == child.id()));
    }
}
