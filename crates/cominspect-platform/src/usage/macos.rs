//! macOS: the open files the kernel reports for each process (`libproc`, the
//! mechanism `lsof` uses). Reading them never touches the device.

use std::collections::{BTreeMap, HashMap};
use std::ffi::{CStr, c_int, c_void};
use std::path::Path;

use super::{PortHolder, PortUsage, UsageSnapshot};

const PROC_PIDLISTFDS: c_int = 1;
const PROX_FDTYPE_VNODE: u32 = 1;
const PROC_PIDFDVNODEPATHINFO: c_int = 2;
const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;
/// `sizeof(struct vnode_fdinfowithpath)`: `proc_fileinfo` (24 bytes) +
/// `vnode_info` (152) + `char vip_path[MAXPATHLEN]` (1024).
const VNODE_FDINFOWITHPATH_SIZE: usize = 1200;
/// Offset of `vip_path` within `struct vnode_fdinfowithpath`.
const VIP_PATH_OFFSET: usize = 24 + 152;
const MAXPATHLEN: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcFdInfo {
    proc_fd: i32,
    proc_fdtype: u32,
}

unsafe extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        arg: u64,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;
    fn proc_pidfdinfo(
        pid: c_int,
        fd: c_int,
        flavor: c_int,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
}

pub struct Probe {
    fds: Vec<ProcFdInfo>,
    path_buf: Vec<u64>,
}

impl Probe {
    pub fn new() -> Probe {
        Probe {
            fds: Vec::new(),
            path_buf: vec![0u64; VNODE_FDINFOWITHPATH_SIZE / 8],
        }
    }

    pub fn check(&mut self, ports: &[String]) -> UsageSnapshot {
        let mut result = BTreeMap::new();
        // Path a program may have opened -> requested port names. A port is
        // reachable as /dev/cu.X (call-out) and /dev/tty.X (call-in).
        let mut targets: HashMap<String, Vec<String>> = HashMap::new();
        for port in ports {
            if !Path::new(port).exists() {
                result.insert(
                    port.clone(),
                    PortUsage::Unknown {
                        reason: "the port does not exist".into(),
                    },
                );
                continue;
            }
            let mut paths = vec![port.clone()];
            if let Some(rest) = port.strip_prefix("/dev/cu.") {
                paths.push(format!("/dev/tty.{rest}"));
            } else if let Some(rest) = port.strip_prefix("/dev/tty.") {
                paths.push(format!("/dev/cu.{rest}"));
            }
            for path in paths {
                targets.entry(path).or_default().push(port.clone());
            }
        }

        let mut holders: HashMap<String, Vec<PortHolder>> = HashMap::new();
        if !targets.is_empty() {
            for pid in all_pids() {
                let mut holder: Option<PortHolder> = None;
                for path in self.vnode_paths(pid) {
                    let Some(names) = targets.get(&path) else {
                        continue;
                    };
                    let holder = holder.get_or_insert_with(|| describe(pid));
                    for name in names {
                        let list = holders.entry(name.clone()).or_default();
                        if !list.iter().any(|h| h.pid == holder.pid) {
                            list.push(holder.clone());
                        }
                    }
                }
            }
        }

        for names in targets.values() {
            for name in names {
                if !result.contains_key(name) {
                    let found = holders.remove(name).unwrap_or_default();
                    result.insert(name.clone(), PortUsage::in_use(found));
                }
            }
        }

        // SAFETY: geteuid has no preconditions.
        let root = unsafe { libc::geteuid() } == 0;
        UsageSnapshot {
            ports: result,
            limitation: (!root).then(|| {
                "Programs run by other users are not visible unless ComInspect runs as root.".into()
            }),
            duration_ms: 0,
        }
    }

    /// Paths of the files and devices a process has open (empty when the
    /// process cannot be inspected).
    fn vnode_paths(&mut self, pid: c_int) -> Vec<String> {
        // SAFETY: a null buffer asks for the size in bytes.
        let bytes = unsafe { proc_pidinfo(pid, PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
        if bytes <= 0 {
            return Vec::new();
        }
        let capacity = bytes as usize / size_of::<ProcFdInfo>() + 16;
        self.fds.clear();
        self.fds.resize(capacity, ProcFdInfo::default());
        // SAFETY: the buffer holds `capacity` entries.
        let bytes = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDLISTFDS,
                0,
                self.fds.as_mut_ptr().cast(),
                (capacity * size_of::<ProcFdInfo>()) as c_int,
            )
        };
        if bytes <= 0 {
            return Vec::new();
        }
        let count = (bytes as usize / size_of::<ProcFdInfo>()).min(capacity);
        let mut paths = Vec::new();
        for fd in &self.fds[..count] {
            if fd.proc_fdtype != PROX_FDTYPE_VNODE {
                continue;
            }
            // SAFETY: the buffer is VNODE_FDINFOWITHPATH_SIZE bytes, 8-byte
            // aligned.
            let written = unsafe {
                proc_pidfdinfo(
                    pid,
                    fd.proc_fd,
                    PROC_PIDFDVNODEPATHINFO,
                    self.path_buf.as_mut_ptr().cast(),
                    VNODE_FDINFOWITHPATH_SIZE as c_int,
                )
            };
            if written as usize != VNODE_FDINFOWITHPATH_SIZE {
                continue;
            }
            // SAFETY: path_buf is VNODE_FDINFOWITHPATH_SIZE bytes long.
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    self.path_buf.as_ptr().cast::<u8>(),
                    VNODE_FDINFOWITHPATH_SIZE,
                )
            };
            let raw = &bytes[VIP_PATH_OFFSET..VIP_PATH_OFFSET + MAXPATHLEN];
            if let Ok(path) = CStr::from_bytes_until_nul(raw)
                && !path.is_empty()
            {
                paths.push(path.to_string_lossy().into_owned());
            }
        }
        paths
    }
}

fn all_pids() -> Vec<c_int> {
    // SAFETY: a null buffer asks for the number of processes.
    let count = unsafe { proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return Vec::new();
    }
    let mut pids = vec![0 as c_int; count as usize + 64];
    // SAFETY: the buffer holds pids.len() entries.
    let filled = unsafe {
        proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * size_of::<c_int>()) as c_int,
        )
    };
    if filled <= 0 {
        return Vec::new();
    }
    pids.truncate(filled as usize);
    pids.retain(|&pid| pid > 0);
    pids
}

fn describe(pid: c_int) -> PortHolder {
    let mut buf = vec![0u8; PROC_PIDPATHINFO_MAXSIZE];
    // SAFETY: the buffer is PROC_PIDPATHINFO_MAXSIZE bytes.
    let len = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    let executable = (len > 0)
        .then(|| String::from_utf8_lossy(&buf[..len as usize]).into_owned())
        .filter(|p| !p.is_empty());
    let process_name = executable
        .as_deref()
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .or_else(|| {
            let mut name = [0u8; 256];
            // SAFETY: the buffer is 256 bytes.
            let len = unsafe { proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
            (len > 0).then(|| String::from_utf8_lossy(&name[..len as usize]).into_owned())
        })
        .unwrap_or_else(|| format!("process {pid}"));
    PortHolder {
        pid: pid as u32,
        process_name,
        executable,
        description: None,
        exiting: false,
    }
}
