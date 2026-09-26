//! `cominspect-cli` — headless access to ComInspect's discovery, inventory and
//! diagnostics. Used for troubleshooting, scripting and CI smoke tests.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use cominspect_core::analysis::analyze_scan;
use cominspect_core::model::ScanResult;
use cominspect_platform::cat::CatProtocol;
use cominspect_platform::diagnostics::{self, FlowSetting, ParitySetting, SerialSettings};
use cominspect_platform::monitor::{Monitor, MonitorConfig};
use cominspect_store::{Inventory, Store};

const USAGE: &str = "\
ComInspect command-line tool

USAGE:
    cominspect-cli <COMMAND> [OPTIONS]

COMMANDS:
    list [--json]                 Scan and list serial ports (read-only, never opens a port)
    watch [--json] [--verbose]    Monitor ports and print arrivals/removals until Ctrl+C
    inventory [--db PATH] [--json]
                                  Reconcile a scan with the device database and print it
                                  (default database: the ComInspect app's database)
    export [--db PATH] --out FILE Export nicknames and identities to a JSON file
    db-path                       Print the default database location
    test-open PORT                Check whether PORT can be opened (may toggle DTR/RTS!)
    cat PORT --protocol P [--baud N|auto] [--stop-bits 1|2] [--civ HEX]
                                  Send one read-only CAT query. P is one of:
                                  kenwood-id, kenwood-frequency, icom-id, icom-frequency,
                                  yaesu-legacy-frequency
    version                       Print the version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        return ExitCode::from(2);
    };
    let rest = &args[1..];
    let result = match command {
        "list" => cmd_list(rest),
        "watch" => cmd_watch(rest),
        "inventory" => cmd_inventory(rest),
        "export" => cmd_export(rest),
        "db-path" => {
            println!("{}", default_db_path().display());
            Ok(())
        }
        "test-open" => cmd_test_open(rest),
        "cat" => cmd_cat(rest),
        "version" | "--version" | "-V" => {
            println!("cominspect-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command '{other}'\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn positional(args: &[String]) -> Option<String> {
    args.iter().find(|a| !a.starts_with("--")).cloned()
}

/// Same location the desktop app uses (`app_local_data_dir`).
fn default_db_path() -> PathBuf {
    const IDENTIFIER: &str = "io.github.kk4oda.cominspect";
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.unwrap_or_else(|| PathBuf::from("."))
        .join(IDENTIFIER)
        .join("inventory.db")
}

fn hex_id(vid: Option<u16>, pid: Option<u16>) -> String {
    match (vid, pid) {
        (Some(v), Some(p)) => format!("{v:04X}:{p:04X}"),
        _ => "—".into(),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut t: String = text.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn print_scan(scan: &ScanResult) {
    println!(
        "{} serial port(s) on {} (scan took {} ms)",
        scan.ports.len(),
        scan.platform,
        scan.duration_ms
    );
    println!(
        "{:<24} {:<8} {:<9} {:<9} {:<18} DEVICE",
        "PORT", "STATE", "TYPE", "VID:PID", "SERIAL"
    );
    for p in &scan.ports {
        let usb = p.usb.as_ref();
        println!(
            "{:<24} {:<8} {:<9} {:<9} {:<18} {}",
            truncate(&p.port_name, 24),
            if p.is_present() { "present" } else { "hidden" },
            p.transport.label(),
            hex_id(usb.map(|u| u.vid), usb.map(|u| u.pid)),
            truncate(
                usb.and_then(|u| u.serial_number.as_deref()).unwrap_or("—"),
                18
            ),
            p.device_label()
        );
    }
    if let Some(reserved) = &scan.reserved {
        let list: Vec<String> = reserved.numbers.iter().map(|n| format!("COM{n}")).collect();
        println!(
            "\nReserved COM numbers ({}): {}",
            reserved.source,
            list.join(", ")
        );
    }
    let findings = analyze_scan(scan);
    if !findings.is_empty() {
        println!("\nFindings:");
        for f in findings {
            println!("  [{:?}] {} — {}", f.severity, f.title, f.detail);
        }
    }
    for w in &scan.warnings {
        println!("warning: {w}");
    }
}

fn cmd_list(args: &[String]) -> Result<(), String> {
    let scan = cominspect_platform::discover();
    if flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&scan).map_err(|e| e.to_string())?
        );
    } else {
        print_scan(&scan);
    }
    Ok(())
}

fn cmd_watch(args: &[String]) -> Result<(), String> {
    let json = flag(args, "--json");
    let verbose = flag(args, "--verbose");
    let store = Store::open_in_memory().map_err(|e| e.to_string())?;
    let mut inventory = Inventory::new(store, cominspect_platform::platform_name());
    let (tx, rx) = mpsc::channel();
    let _monitor = Monitor::start(
        MonitorConfig::for_current_platform(),
        move |scan, trigger| {
            let _ = tx.send((scan, trigger));
        },
    );
    eprintln!("Watching serial ports. Press Ctrl+C to stop.");
    while let Ok((scan, trigger)) = rx.recv() {
        let outcome = inventory.reconcile(scan).map_err(|e| e.to_string())?;
        for event in outcome.events {
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&event).map_err(|e| e.to_string())?
                );
            } else {
                let port = event.port.as_deref().unwrap_or("?");
                let was = event
                    .previous_port
                    .as_deref()
                    .map(|p| format!(" (was {p})"))
                    .unwrap_or_default();
                println!(
                    "{:<13} {:<24} {}{was}{}",
                    event.kind.as_str(),
                    port,
                    event.label,
                    if event.initial { "  [startup]" } else { "" }
                );
            }
        }
        if verbose {
            eprintln!("scan ({trigger:?})");
        }
    }
    Ok(())
}

fn open_inventory(args: &[String]) -> Result<Inventory, String> {
    let path = option(args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(default_db_path);
    let (store, report) =
        Store::open(&path, env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
    if let Some(reason) = &report.temporary_reason {
        eprintln!("warning: {reason}");
    }
    Ok(Inventory::new(store, cominspect_platform::platform_name()))
}

fn cmd_inventory(args: &[String]) -> Result<(), String> {
    let mut inventory = open_inventory(args)?;
    inventory
        .reconcile(cominspect_platform::discover())
        .map_err(|e| e.to_string())?;
    let view = inventory.view().map_err(|e| e.to_string())?;
    if flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&view).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    println!(
        "{:<3} {:<24} {:<28} {:<9} {:<9} DEVICE",
        "", "PORT", "NICKNAME", "TYPE", "VID:PID"
    );
    for row in view.rows.iter().filter(|r| !r.ignored) {
        println!(
            "{:<3} {:<24} {:<28} {:<9} {:<9} {}",
            if row.status.is_connected() {
                "●"
            } else {
                "○"
            },
            truncate(row.port.as_deref().unwrap_or("—"), 24),
            truncate(row.nickname.as_deref().unwrap_or("—"), 28),
            row.transport.label(),
            hex_id(row.vid, row.pid),
            row.device_label
        );
    }
    println!(
        "\n{} devices · {} connected · {} disconnected",
        view.summary.total, view.summary.connected, view.summary.disconnected
    );
    Ok(())
}

fn cmd_export(args: &[String]) -> Result<(), String> {
    let out = option(args, "--out").ok_or("--out FILE is required")?;
    let inventory = open_inventory(args)?;
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok();
    let doc = inventory
        .export(env!("CARGO_PKG_VERSION"), hostname.as_deref())
        .map_err(|e| e.to_string())?;
    std::fs::write(&out, doc.to_json_pretty()).map_err(|e| e.to_string())?;
    println!("Exported {} devices to {out}", doc.devices.len());
    Ok(())
}

fn cmd_test_open(args: &[String]) -> Result<(), String> {
    let port = positional(args).ok_or("PORT is required")?;
    let report = diagnostics::open_test(&port);
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn cmd_cat(args: &[String]) -> Result<(), String> {
    let port = positional(args).ok_or("PORT is required")?;
    let protocol = match option(args, "--protocol").as_deref() {
        Some("kenwood-id") => CatProtocol::KenwoodId,
        Some("kenwood-frequency") => CatProtocol::KenwoodFrequency,
        Some("icom-id") => CatProtocol::IcomId,
        Some("icom-frequency") => CatProtocol::IcomFrequency,
        Some("yaesu-legacy-frequency") => CatProtocol::YaesuLegacyFrequency,
        _ => return Err("--protocol is required (see help)".into()),
    };
    let info = cominspect_platform::cat::PROBES
        .iter()
        .find(|p| p.protocol == protocol)
        .expect("probe metadata");
    let baud = match option(args, "--baud").as_deref() {
        None | Some("auto") => 0,
        Some(b) => b.parse::<u32>().map_err(|_| "invalid --baud")?,
    };
    let stop_bits = option(args, "--stop-bits")
        .map(|b| b.parse::<u8>().map_err(|_| "invalid --stop-bits"))
        .transpose()?
        .unwrap_or(info.default_stop_bits);
    let civ = option(args, "--civ")
        .map(|c| u8::from_str_radix(c.trim_start_matches("0x"), 16).map_err(|_| "invalid --civ"))
        .transpose()?
        .unwrap_or(0x00);
    let settings = SerialSettings {
        baud_rate: baud,
        data_bits: 8,
        parity: ParitySetting::None,
        stop_bits,
        flow_control: FlowSetting::None,
    };
    let report =
        diagnostics::cat_query(&port, &settings, protocol, civ, Duration::from_millis(1000));
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}
