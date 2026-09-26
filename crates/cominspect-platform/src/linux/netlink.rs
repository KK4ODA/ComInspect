//! Kernel uevent listener (`NETLINK_KOBJECT_UEVENT`). Unprivileged processes
//! may subscribe to kernel uevents, so no udev library or daemon is needed.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use crate::monitor::{ChangeHint, HintSource};

/// Subsystems whose events can add or remove serial ports.
const RELEVANT_SUBSYSTEMS: &[&str] = &["tty", "usb", "usb-serial", "bluetooth", "pnp", "pci"];

fn open_socket() -> io::Result<OwnedFd> {
    // SAFETY: plain socket(2) call; the result is checked before use.
    let fd = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
            libc::NETLINK_KOBJECT_UEVENT,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is a freshly created, owned descriptor.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: zero is a valid bit pattern for sockaddr_nl.
    let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    addr.nl_family = libc::AF_NETLINK as libc::sa_family_t;
    addr.nl_pid = 0;
    addr.nl_groups = 1; // kernel uevents
    // SAFETY: addr is a valid sockaddr_nl and the length matches.
    let rc = unsafe {
        libc::bind(
            owned.as_raw_fd(),
            (&addr as *const libc::sockaddr_nl).cast(),
            std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(owned)
}

/// Extracts `SUBSYSTEM=` from a raw uevent datagram
/// (`ACTION@DEVPATH\0KEY=VALUE\0…`).
pub(crate) fn uevent_subsystem(data: &[u8]) -> Option<&str> {
    data.split(|b| *b == 0)
        .filter_map(|field| std::str::from_utf8(field).ok())
        .find_map(|field| field.strip_prefix("SUBSYSTEM="))
}

struct NetlinkSource {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl HintSource for NetlinkSource {
    fn name(&self) -> &'static str {
        "netlink"
    }
}

impl Drop for NetlinkSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub(crate) fn start_sources(tx: Sender<ChangeHint>) -> Vec<Box<dyn HintSource>> {
    let socket = match open_socket() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("netlink uevents unavailable ({e}); relying on polling");
            return Vec::new();
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let thread = thread::Builder::new()
        .name("uevent-listener".into())
        .spawn(move || {
            let mut buf = vec![0u8; 16 * 1024];
            while !stop_flag.load(Ordering::SeqCst) {
                let mut pfd = libc::pollfd {
                    fd: socket.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: pfd points to one valid pollfd.
                let ready = unsafe { libc::poll(&mut pfd, 1, 500) };
                if ready <= 0 {
                    continue;
                }
                // SAFETY: buf is valid for buf.len() bytes.
                let n = unsafe {
                    libc::recv(socket.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0)
                };
                if n <= 0 {
                    continue;
                }
                let data = &buf[..n as usize];
                if let Some(subsystem) = uevent_subsystem(data)
                    && RELEVANT_SUBSYSTEMS.contains(&subsystem)
                    && tx.send(ChangeHint::Os("uevent")).is_err()
                {
                    break;
                }
            }
        });
    match thread {
        Ok(handle) => vec![Box::new(NetlinkSource {
            stop,
            thread: Some(handle),
        })],
        Err(e) => {
            log::warn!("could not start uevent listener: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subsystem_from_uevent() {
        let msg = b"add@/devices/pci0000:00/usb1/1-4/1-4:1.0/ttyUSB0/tty/ttyUSB0\0ACTION=add\0DEVPATH=/x\0SUBSYSTEM=tty\0MAJOR=188\0";
        assert_eq!(uevent_subsystem(msg), Some("tty"));
        assert_eq!(uevent_subsystem(b"libudev\0garbage"), None);
    }
}
