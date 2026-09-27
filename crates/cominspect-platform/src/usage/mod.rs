//! Which programs have a serial port open, found **without opening the
//! port**. Opening a port can assert RTS/DTR, which keys many transmitters,
//! and would race with the program that is about to use the port.
//!
//! * Windows: the system handle table, matching file handles to the device
//!   behind the port name, like Sysinternals Process Explorer
//!   ([`windows`](self) module docs explain the safeguards).
//! * Linux: the `/proc/<pid>/fd` links.
//! * macOS: the open files the kernel reports for each process.
//!
//! Only processes the current user may inspect are visible; the snapshot's
//! [`limitation`](UsageSnapshot::limitation) says what that leaves out.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

pub use cominspect_core::usage::{PortHolder, PortUsage, UsageChange, UsageSnapshot, changes};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
use unsupported as imp;

/// Checks port usage repeatedly; keeps caches and buffers between checks, so
/// reuse one probe for periodic checks.
pub struct UsageProbe {
    inner: imp::Probe,
}

impl Default for UsageProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageProbe {
    pub fn new() -> Self {
        UsageProbe {
            inner: imp::Probe::new(),
        }
    }

    /// Checks the given ports (the names applications open: `COM7`,
    /// `/dev/ttyUSB0`). Every requested port appears in the result. Takes a
    /// few milliseconds up to a few tens of milliseconds; call it off the UI
    /// thread.
    pub fn check(&mut self, ports: &[String]) -> UsageSnapshot {
        let started = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| self.inner.check(ports)));
        let mut snapshot = match outcome {
            Ok(snapshot) => snapshot,
            Err(_) => {
                log::error!("port usage check panicked");
                // Start over with fresh state next time.
                self.inner = imp::Probe::new();
                UsageSnapshot {
                    ports: ports
                        .iter()
                        .map(|p| {
                            (
                                p.clone(),
                                PortUsage::Unknown {
                                    reason: "internal error while checking".into(),
                                },
                            )
                        })
                        .collect(),
                    ..Default::default()
                }
            }
        };
        for port in ports {
            snapshot
                .ports
                .entry(port.clone())
                .or_insert_with(|| PortUsage::Unknown {
                    reason: "not checked".into(),
                });
        }
        snapshot.duration_ms = started.elapsed().as_millis() as u64;
        snapshot
    }
}

/// One-off check of a single port.
pub fn port_usage(port: &str) -> PortUsage {
    let port = port.to_string();
    UsageProbe::new()
        .check(std::slice::from_ref(&port))
        .ports
        .remove(&port)
        .unwrap_or(PortUsage::Unknown {
            reason: "not checked".into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_requested_port_is_reported() {
        let ports = vec![if cfg!(windows) {
            "COM250".to_string()
        } else {
            "/dev/does-not-exist-cominspect".to_string()
        }];
        let snapshot = UsageProbe::new().check(&ports);
        assert_eq!(snapshot.ports.len(), 1);
        assert!(!snapshot.ports[&ports[0]].is_in_use());
    }

    /// A pseudo-terminal held open by a child process is reported with the
    /// child's process ID, and reported free once the child exits.
    #[cfg(unix)]
    #[test]
    fn detects_a_child_holding_a_terminal() {
        use std::process::{Command, Stdio};
        use std::time::Duration;

        // SAFETY: plain libc calls with valid out-pointers.
        let (master, slave) = unsafe {
            let mut master = 0;
            let mut slave = 0;
            assert_eq!(
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut()
                ),
                0
            );
            (master, slave)
        };
        // SAFETY: slave is a valid descriptor from openpty.
        let name = unsafe { std::ffi::CStr::from_ptr(libc::ttyname(slave)) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: closing descriptors we own.
        unsafe {
            libc::close(slave);
        }
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(format!("exec 3<>'{name}'; exec sleep 30"))
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let mut probe = UsageProbe::new();
        let ports = vec![name.clone()];
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut usage = PortUsage::Free;
        while Instant::now() < deadline {
            usage = probe.check(&ports).ports.remove(&name).unwrap();
            if usage.holders().iter().any(|h| h.pid == child.id()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let holder = usage
            .holders()
            .iter()
            .find(|h| h.pid == child.id())
            .cloned();
        let _ = child.kill();
        let _ = child.wait();
        let holder = holder.unwrap_or_else(|| panic!("child not found holding {name}: {usage:?}"));
        assert!(
            holder.process_name.contains("sh") || holder.process_name.contains("sleep"),
            "unexpected process name {holder:?}"
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut after = probe.check(&ports).ports.remove(&name).unwrap();
        while after.holders().iter().any(|h| h.pid == child.id()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
            after = probe.check(&ports).ports.remove(&name).unwrap();
        }
        // SAFETY: closing the descriptor we own.
        unsafe {
            libc::close(master);
        }
        assert!(
            !after.holders().iter().any(|h| h.pid == child.id()),
            "still reported after the child exited: {after:?}"
        );
    }
}
