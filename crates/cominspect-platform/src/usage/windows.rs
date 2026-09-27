//! Windows: processes holding a handle to the device behind a port name,
//! found through the system handle table without opening the port.
//!
//! 1. `QueryDosDeviceW("COM4")` names the device behind the port
//!    (`\Device\Silabser0`).
//! 2. `NtQuerySystemInformation(SystemExtendedHandleInformation)` lists every
//!    open handle with its process and object type.
//! 3. File handles with read or write access, in processes this user may
//!    inspect, are duplicated into ComInspect and their object name read
//!    (`NtQueryObject(ObjectNameInformation)`). A handle named like the
//!    port's device belongs to a program using the port.
//!
//! A duplicated handle refers to the other program's existing open: the
//! serial driver is not called and RTS/DTR do not change.
//!
//! Hazard: reading the name of a file opened for synchronous I/O waits while
//! another thread of the owning program is inside synchronous I/O on it (a
//! `ReadFile` blocked on a pipe, for example), possibly forever. Names are
//! therefore read on a worker thread. When a query takes longer than
//! [`STALL_AFTER`], the duplicate is closed at once, so ComInspect never
//! keeps anyone's port open (the stalled query holds only an object
//! reference; when the owner closes its handle, its pending I/O is cancelled
//! and the query finishes), a new worker continues, and the stalled answer
//! is used by a later check once it arrives.
//!
//! Answers are cached per handle for a few checks (a handle value cannot be
//! reused while the handle is open). Before a port is reported released, the
//! previous holders' handles are checked again without the cache, so a
//! program that reopened the port under a recycled handle value is still
//! seen.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Wdk::Foundation::{NtQueryObject, OBJECT_NAME_INFORMATION};
use windows_sys::Wdk::System::SystemInformation::NtQuerySystemInformation;
use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, GENERIC_READ, HANDLE,
    INVALID_HANDLE_VALUE, STATUS_INFO_LENGTH_MISMATCH,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileVersionInfoSizeW, GetFileVersionInfoW,
    OPEN_EXISTING, QueryDosDeviceW, VerQueryValueW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_DUP_HANDLE,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

use super::{PortHolder, PortUsage, UsageSnapshot};

const SYSTEM_EXTENDED_HANDLE_INFORMATION: i32 = 64;
const OBJECT_NAME_INFORMATION_CLASS: i32 = 1;
const FILE_READ_DATA: u32 = 0x0001;
const FILE_WRITE_DATA: u32 = 0x0002;
/// A name query that has not returned after this long is treated as stalled.
const STALL_AFTER: Duration = Duration::from_millis(100);
/// While this many workers are stuck, no new queries are started.
const MAX_STUCK_WORKERS: usize = 32;
/// Cached answers are refreshed after this many checks.
const RECHECK_AFTER: u64 = 10;
/// The handle table is refused beyond this size.
const MAX_TABLE_BYTES: usize = 512 << 20;

/// `SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX`.
#[repr(C)]
#[derive(Clone, Copy)]
struct HandleEntry {
    object: usize,
    pid: usize,
    handle: usize,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

/// `SYSTEM_HANDLE_INFORMATION_EX` header: `NumberOfHandles`, `Reserved`.
const TABLE_HEADER: usize = 2 * size_of::<usize>();

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct HandleKey {
    pid: u32,
    handle: usize,
    access: u32,
    /// Kernel object address; zero where Windows hides it.
    object: usize,
}

struct Cached {
    /// Lowercase `\device\name` for a handle opened on a device itself (not
    /// a file on it); `None` for everything else.
    device: Option<String>,
    at: u64,
}

struct Job {
    id: u64,
    key: HandleKey,
    dup: usize,
}

/// Reads object names on its own thread so a stalled query cannot block the
/// check.
struct Worker {
    jobs: Sender<Job>,
    results: Receiver<(u64, Option<String>)>,
    started: Arc<AtomicU64>,
    abandoned: Arc<AtomicBool>,
    next_id: u64,
}

impl Worker {
    fn spawn(
        late: Arc<Mutex<HashMap<HandleKey, Option<String>>>>,
        stuck: Arc<AtomicUsize>,
    ) -> Option<Worker> {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (result_tx, result_rx) = mpsc::channel();
        let started = Arc::new(AtomicU64::new(0));
        let abandoned = Arc::new(AtomicBool::new(false));
        let (worker_started, worker_abandoned) = (started.clone(), abandoned.clone());
        thread::Builder::new()
            .name("cominspect-usage-query".into())
            .spawn(move || {
                while let Ok(job) = job_rx.recv() {
                    if worker_abandoned.load(Ordering::SeqCst) {
                        break;
                    }
                    worker_started.store(job.id, Ordering::SeqCst);
                    let name = device_name(job.dup as HANDLE);
                    if worker_abandoned.load(Ordering::SeqCst) {
                        // The check gave up waiting and already closed the
                        // duplicate; leave the answer for the next check.
                        if let Ok(mut late) = late.lock() {
                            late.insert(job.key, name);
                        }
                        break;
                    }
                    if result_tx.send((job.id, name)).is_err() {
                        break;
                    }
                }
                if worker_abandoned.load(Ordering::SeqCst) {
                    stuck.fetch_sub(1, Ordering::SeqCst);
                }
            })
            .ok()?;
        Some(Worker {
            jobs: job_tx,
            results: result_rx,
            started,
            abandoned,
            next_id: 1,
        })
    }
}

enum Query {
    Done(Option<String>),
    Stalled,
    Failed,
}

/// Process handle opened for inspection; closed on drop.
struct ProcessHandle {
    handle: HANDLE,
    own: bool,
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.own {
            // SAFETY: a handle this struct opened.
            unsafe { CloseHandle(self.handle) };
        }
    }
}

pub struct Probe {
    table: Vec<u64>,
    file_type: Option<u16>,
    pass: u64,
    cache: HashMap<HandleKey, Cached>,
    stalled: HashSet<HandleKey>,
    late: Arc<Mutex<HashMap<HandleKey, Option<String>>>>,
    stuck: Arc<AtomicUsize>,
    worker: Option<Worker>,
    /// Port -> processes that held it at the previous check.
    last_holders: HashMap<String, HashSet<u32>>,
    /// Executable path -> file description.
    descriptions: HashMap<String, Option<String>>,
    elevated: bool,
    self_pid: u32,
}

impl Probe {
    pub fn new() -> Probe {
        Probe {
            table: Vec::new(),
            file_type: None,
            pass: 0,
            cache: HashMap::new(),
            stalled: HashSet::new(),
            late: Arc::new(Mutex::new(HashMap::new())),
            stuck: Arc::new(AtomicUsize::new(0)),
            worker: None,
            last_holders: HashMap::new(),
            descriptions: HashMap::new(),
            elevated: is_elevated(),
            // SAFETY: no preconditions.
            self_pid: unsafe { GetCurrentProcessId() },
        }
    }

    pub fn check(&mut self, ports: &[String]) -> UsageSnapshot {
        self.pass += 1;
        let mut result: BTreeMap<String, PortUsage> = BTreeMap::new();
        // Lowercase device name -> requested ports.
        let mut wanted: HashMap<String, Vec<String>> = HashMap::new();
        for port in ports {
            let devices = dos_device_targets(port);
            if devices.is_empty() {
                result.insert(
                    port.clone(),
                    PortUsage::Unknown {
                        reason: "the port does not exist".into(),
                    },
                );
                continue;
            }
            for device in devices {
                let list = wanted.entry(device.to_lowercase()).or_default();
                if !list.contains(port) {
                    list.push(port.clone());
                }
            }
        }
        let mut incomplete = false;
        if !wanted.is_empty() {
            match self.candidates() {
                Ok(candidates) => {
                    let found = self.find_holders(&candidates, &wanted, &mut incomplete);
                    for ports in wanted.values() {
                        for port in ports {
                            let holders = found.get(port).cloned().unwrap_or_default();
                            result.insert(port.clone(), PortUsage::in_use(holders));
                        }
                    }
                }
                Err(reason) => {
                    for ports in wanted.values() {
                        for port in ports {
                            result.insert(
                                port.clone(),
                                PortUsage::Unknown {
                                    reason: reason.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }
        let mut limitation = (!self.elevated).then(|| {
            "Programs running as administrator or as another user are not visible unless \
             ComInspect also runs as administrator."
                .to_string()
        });
        if incomplete {
            let note = "Some programs could not be checked this time.";
            limitation = Some(match limitation {
                Some(l) => format!("{l} {note}"),
                None => note.into(),
            });
        }
        UsageSnapshot {
            ports: result,
            limitation,
            duration_ms: 0,
        }
    }

    /// File handles with read or write access in all processes except the
    /// kernel's.
    fn candidates(&mut self) -> Result<Vec<HandleKey>, String> {
        let nul = if self.file_type.is_none() {
            open_nul()
        } else {
            None
        };
        let self_pid = self.self_pid as usize;
        let known_type = self.file_type;
        let outcome = self.handle_table().map(|entries| {
            let found = nul.and_then(|nul| {
                entries
                    .iter()
                    .find(|e| e.pid == self_pid && e.handle == nul as usize)
                    .map(|e| e.object_type_index)
            });
            let keys = found.or(known_type).map(|file_type| {
                entries
                    .iter()
                    .filter(|e| {
                        e.object_type_index == file_type
                            && e.pid > 4
                            && e.granted_access & (FILE_READ_DATA | FILE_WRITE_DATA) != 0
                    })
                    .map(|e| HandleKey {
                        pid: e.pid as u32,
                        handle: e.handle,
                        access: e.granted_access,
                        object: e.object,
                    })
                    .collect::<Vec<_>>()
            });
            (found, keys)
        });
        if let Some(nul) = nul {
            // SAFETY: a handle opened by open_nul.
            unsafe { CloseHandle(nul) };
        }
        let (found, keys) = outcome?;
        if found.is_some() {
            self.file_type = found;
        }
        let Some(keys) = keys else {
            return Err("file handles could not be identified".into());
        };
        let present: HashSet<HandleKey> = keys.iter().copied().collect();
        self.cache.retain(|k, _| present.contains(k));
        self.stalled.retain(|k| present.contains(k));
        if let Ok(mut late) = self.late.lock() {
            for (key, name) in late.drain() {
                self.stalled.remove(&key);
                if present.contains(&key) {
                    self.cache.insert(
                        key,
                        Cached {
                            device: name,
                            at: self.pass,
                        },
                    );
                }
            }
        }
        Ok(keys)
    }

    fn find_holders(
        &mut self,
        candidates: &[HandleKey],
        wanted: &HashMap<String, Vec<String>>,
        incomplete: &mut bool,
    ) -> HashMap<String, Vec<PortHolder>> {
        let mut by_pid: BTreeMap<u32, Vec<HandleKey>> = BTreeMap::new();
        for key in candidates {
            by_pid.entry(key.pid).or_default().push(*key);
        }
        let mut ports_by_pid: HashMap<u32, HashSet<String>> = HashMap::new();
        for (pid, keys) in &by_pid {
            let ports = self.classify_process(*pid, keys, wanted, false, incomplete);
            if !ports.is_empty() {
                ports_by_pid.insert(*pid, ports);
            }
        }

        // Before reporting a port released, look again at whoever held it.
        let mut suspects = HashSet::new();
        for (port, pids) in &self.last_holders {
            let still_held = ports_by_pid.values().any(|ports| ports.contains(port));
            if !still_held && wanted.values().any(|ports| ports.contains(port)) {
                suspects.extend(pids.iter().copied());
            }
        }
        for pid in suspects {
            if let Some(keys) = by_pid.get(&pid) {
                let ports = self.classify_process(pid, keys, wanted, true, incomplete);
                if !ports.is_empty() {
                    ports_by_pid.entry(pid).or_default().extend(ports);
                }
            }
        }

        let mut holders: HashMap<String, Vec<PortHolder>> = HashMap::new();
        self.last_holders.clear();
        for (pid, ports) in ports_by_pid {
            let holder = self.describe(pid);
            for port in ports {
                self.last_holders
                    .entry(port.clone())
                    .or_default()
                    .insert(pid);
                holders.entry(port).or_default().push(holder.clone());
            }
        }
        holders
    }

    /// Ports a process has open. `fresh` ignores cached answers.
    fn classify_process(
        &mut self,
        pid: u32,
        keys: &[HandleKey],
        wanted: &HashMap<String, Vec<String>>,
        fresh: bool,
        incomplete: &mut bool,
    ) -> HashSet<String> {
        let mut devices: Vec<String> = Vec::new();
        let mut to_query = Vec::new();
        for key in keys {
            let cached = self.cache.get(key);
            if self.stalled.contains(key) {
                // Still waiting for an earlier answer; the handle is still
                // open, so an earlier answer is still right.
                match cached.and_then(|c| c.device.clone()) {
                    Some(device) => devices.push(device),
                    None => *incomplete |= cached.is_none(),
                }
                continue;
            }
            match cached {
                Some(c) if !fresh && self.pass.saturating_sub(c.at) < RECHECK_AFTER => {
                    if let Some(device) = &c.device {
                        devices.push(device.clone());
                    }
                }
                _ => to_query.push(*key),
            }
        }

        if !to_query.is_empty() {
            match self.open_process(pid, PROCESS_DUP_HANDLE) {
                Some(process) => {
                    for key in to_query {
                        if self.stuck.load(Ordering::SeqCst) >= MAX_STUCK_WORKERS {
                            *incomplete = true;
                            break;
                        }
                        match self.query_handle(&process, key) {
                            Some(Some(device)) => devices.push(device),
                            Some(None) => {}
                            None => {
                                if let Some(Some(device)) =
                                    self.cache.get(&key).map(|c| c.device.clone())
                                {
                                    devices.push(device);
                                }
                            }
                        }
                    }
                }
                None => {
                    // Not inspectable (another user, or elevated while we are
                    // not): covered by the limitation note.
                }
            }
        }

        let mut ports = HashSet::new();
        for device in devices {
            if let Some(names) = wanted.get(&device) {
                ports.extend(names.iter().cloned());
            }
        }
        ports
    }

    /// Duplicates one handle and reads its device name. `None` when the
    /// query stalled (the answer arrives later).
    fn query_handle(&mut self, process: &ProcessHandle, key: HandleKey) -> Option<Option<String>> {
        let mut dup: HANDLE = std::ptr::null_mut();
        // SAFETY: valid process handles; the duplicate is closed below.
        let ok = unsafe {
            DuplicateHandle(
                process.handle,
                key.handle as HANDLE,
                GetCurrentProcess(),
                &mut dup,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 || dup.is_null() {
            // Closed in the meantime, or not duplicable.
            self.cache.insert(
                key,
                Cached {
                    device: None,
                    at: self.pass,
                },
            );
            return Some(None);
        }
        let outcome = self.run_query(key, dup);
        // Closed in every case: after a stall the worker needs only the
        // object reference it already holds.
        // SAFETY: the duplicate created above.
        unsafe { CloseHandle(dup) };
        match outcome {
            Query::Done(device) => {
                self.cache.insert(
                    key,
                    Cached {
                        device: device.clone(),
                        at: self.pass,
                    },
                );
                Some(device)
            }
            Query::Stalled => {
                self.stalled.insert(key);
                None
            }
            Query::Failed => Some(None),
        }
    }

    fn run_query(&mut self, key: HandleKey, dup: HANDLE) -> Query {
        if self.worker.is_none() {
            self.worker = Worker::spawn(self.late.clone(), self.stuck.clone());
        }
        let Some(worker) = self.worker.as_mut() else {
            return Query::Failed;
        };
        let id = worker.next_id;
        worker.next_id += 1;
        if worker
            .jobs
            .send(Job {
                id,
                key,
                dup: dup as usize,
            })
            .is_err()
        {
            self.worker = None;
            return Query::Failed;
        }
        let started_waiting = Instant::now();
        loop {
            match worker.results.recv_timeout(STALL_AFTER) {
                Ok((done, device)) if done == id => return Query::Done(device),
                Ok(_) => continue,
                Err(RecvTimeoutError::Timeout) => {
                    let started = worker.started.load(Ordering::SeqCst) == id;
                    if !started && started_waiting.elapsed() < Duration::from_secs(2) {
                        // The worker has not picked the job up yet.
                        continue;
                    }
                    worker.abandoned.store(true, Ordering::SeqCst);
                    self.stuck.fetch_add(1, Ordering::SeqCst);
                    self.worker = None;
                    log::debug!(
                        "handle query stalled (pid {}, handle {:#x}); continuing without it",
                        key.pid,
                        key.handle
                    );
                    return Query::Stalled;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.worker = None;
                    return Query::Failed;
                }
            }
        }
    }

    fn open_process(&self, pid: u32, access: u32) -> Option<ProcessHandle> {
        if pid == self.self_pid {
            return Some(ProcessHandle {
                // SAFETY: the pseudo handle needs no closing.
                handle: unsafe { GetCurrentProcess() },
                own: true,
            });
        }
        // SAFETY: plain call; failure returns null.
        let handle = unsafe { OpenProcess(access | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        (!handle.is_null()).then_some(ProcessHandle { handle, own: false })
    }

    fn describe(&mut self, pid: u32) -> PortHolder {
        let executable = self
            .open_process(pid, 0)
            .and_then(|p| process_image(p.handle));
        let process_name = executable
            .as_deref()
            .and_then(|p| p.rsplit(['\\', '/']).next())
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("process {pid}"));
        let description = executable.as_ref().and_then(|path| {
            self.descriptions
                .entry(path.clone())
                .or_insert_with(|| file_description(path))
                .clone()
        });
        PortHolder {
            pid,
            process_name,
            executable,
            description,
        }
    }

    fn handle_table(&mut self) -> Result<&[HandleEntry], String> {
        if self.table.is_empty() {
            self.table = vec![0u64; (4 << 20) / 8];
        }
        loop {
            let bytes = self.table.len() * 8;
            let mut needed = 0u32;
            // SAFETY: the buffer is `bytes` long and 8-byte aligned.
            let status = unsafe {
                NtQuerySystemInformation(
                    SYSTEM_EXTENDED_HANDLE_INFORMATION,
                    self.table.as_mut_ptr().cast(),
                    bytes as u32,
                    &mut needed,
                )
            };
            if status == STATUS_INFO_LENGTH_MISMATCH {
                let next = (needed as usize).max(bytes * 2) + (1 << 20);
                if next > MAX_TABLE_BYTES {
                    return Err("the system handle table is too large".into());
                }
                self.table = vec![0u64; next.div_ceil(8)];
                continue;
            }
            if status < 0 {
                return Err(format!(
                    "the system handle table is not available (0x{:08X})",
                    status as u32
                ));
            }
            let capacity = (bytes - TABLE_HEADER) / size_of::<HandleEntry>();
            let count = (self.table[0] as usize).min(capacity);
            // SAFETY: the kernel wrote `count` entries after the header; the
            // buffer is 8-byte aligned like HandleEntry.
            let entries = unsafe {
                std::slice::from_raw_parts(
                    self.table
                        .as_ptr()
                        .cast::<u8>()
                        .add(TABLE_HEADER)
                        .cast::<HandleEntry>(),
                    count,
                )
            };
            return Ok(entries);
        }
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        // Dropping the worker closes its job queue so its thread ends.
        self.worker = None;
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Devices behind a port name: `COM4` -> `\Device\Silabser0`.
fn dos_device_targets(port: &str) -> Vec<String> {
    let name = port.strip_prefix(r"\\.\").unwrap_or(port);
    if name.is_empty() || name.contains(['\\', '/']) {
        return Vec::new();
    }
    let name = wide(name);
    let mut buf = vec![0u16; 4096];
    // SAFETY: valid NUL-terminated name and buffer.
    let n = unsafe { QueryDosDeviceW(name.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    if n == 0 {
        return Vec::new();
    }
    buf.truncate(n as usize);
    buf.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// The lowercase `\device\name` a handle refers to, when it was opened on a
/// device itself rather than on a file.
fn device_name(handle: HANDLE) -> Option<String> {
    // Device names are short; longer names are files and not wanted.
    let mut buf = [0u64; 128];
    let mut returned = 0u32;
    // SAFETY: the buffer is 1 KiB and 8-byte aligned.
    let status = unsafe {
        NtQueryObject(
            handle,
            OBJECT_NAME_INFORMATION_CLASS,
            buf.as_mut_ptr().cast(),
            size_of_val(&buf) as u32,
            &mut returned,
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: on success the buffer starts with OBJECT_NAME_INFORMATION.
    let info = unsafe { &*buf.as_ptr().cast::<OBJECT_NAME_INFORMATION>() };
    let len = usize::from(info.Name.Length / 2);
    if info.Name.Buffer.is_null() || len == 0 {
        return None;
    }
    // SAFETY: the name lies within the buffer the kernel filled.
    let name =
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(info.Name.Buffer, len) });
    let lower = name.to_lowercase();
    let rest = lower.strip_prefix(r"\device\")?;
    (!rest.is_empty() && !rest.contains('\\')).then_some(lower)
}

fn open_nul() -> Option<HANDLE> {
    let name = wide(r"\\.\NUL");
    // SAFETY: valid name; the handle is closed by the caller.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(handle)
}

fn process_image(process: HANDLE) -> Option<String> {
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: valid buffer and length.
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    (ok != 0 && len > 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// The `FileDescription` from an executable's version resource ("VARA FM").
fn file_description(path: &str) -> Option<String> {
    let path = wide(path);
    let mut ignored = 0u32;
    // SAFETY: valid path.
    let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), &mut ignored) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    // SAFETY: the buffer is `size` bytes.
    if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let query = |sub: &str| -> Option<(*mut c_void, u32)> {
        let sub = wide(sub);
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: data holds the version resource.
        let ok = unsafe { VerQueryValueW(data.as_ptr().cast(), sub.as_ptr(), &mut ptr, &mut len) };
        (ok != 0 && !ptr.is_null() && len > 0).then_some((ptr, len))
    };
    let mut languages = Vec::new();
    if let Some((ptr, len)) = query(r"\VarFileInfo\Translation") {
        let count = len as usize / 4;
        // SAFETY: the translation table holds `count` (language, code page) pairs.
        let pairs = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), count * 2) };
        let (pairs, _) = pairs.as_chunks::<2>();
        for [language, code_page] in pairs {
            languages.push(format!("{language:04x}{code_page:04x}"));
        }
    }
    languages.push("040904b0".into());
    languages.push("040904e4".into());
    for language in languages {
        if let Some((ptr, len)) = query(&format!(r"\StringFileInfo\{language}\FileDescription")) {
            // SAFETY: `len` UTF-16 units including the terminating NUL.
            let text = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), len as usize) };
            let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
            let description = String::from_utf16_lossy(&text[..end]).trim().to_string();
            if !description.is_empty() {
                return Some(description);
            }
        }
    }
    None
}

fn is_elevated() -> bool {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the pseudo handle and an out-pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut returned = 0u32;
    // SAFETY: valid token and buffer.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    // SAFETY: the token opened above.
    unsafe { CloseHandle(token) };
    ok != 0 && elevation.TokenIsElevated != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_entry_layout_matches_the_kernel() {
        assert_eq!(
            size_of::<HandleEntry>(),
            if cfg!(target_pointer_width = "64") {
                40
            } else {
                28
            }
        );
    }

    #[test]
    fn nul_is_a_file_handle_in_the_table() {
        let mut probe = Probe::new();
        probe.candidates().expect("handle table");
        assert!(
            probe.file_type.is_some(),
            "the File object type was not found"
        );
    }

    #[test]
    fn device_names_are_kept_only_for_device_opens() {
        let nul = open_nul().expect("NUL");
        assert_eq!(device_name(nul).as_deref(), Some(r"\device\null"));
        // SAFETY: opened above.
        unsafe { CloseHandle(nul) };
    }

    /// Opens a real serial port when the environment names one
    /// (`COMINSPECT_TEST_PORT=COM2` in CI) and checks that this process is
    /// reported as its holder, and that the port is free after closing it.
    #[test]
    fn detects_this_process_holding_a_port() {
        use std::os::windows::fs::OpenOptionsExt;
        let Ok(port) = std::env::var("COMINSPECT_TEST_PORT") else {
            eprintln!("COMINSPECT_TEST_PORT not set; skipping");
            return;
        };
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(format!(r"\\.\{port}"))
            .unwrap_or_else(|e| panic!("cannot open {port}: {e}"));
        let mut probe = Probe::new();
        let ports = vec![port.clone()];
        let snapshot = probe.check(&ports);
        let usage = &snapshot.ports[&port];
        // SAFETY: no preconditions.
        let me = unsafe { GetCurrentProcessId() };
        assert!(
            usage.holders().iter().any(|h| h.pid == me),
            "this process not reported as holding {port}: {usage:?}"
        );
        drop(file);
        let after = probe.check(&ports);
        assert!(
            !after.ports[&port].holders().iter().any(|h| h.pid == me),
            "{port} still reported in use after closing: {:?}",
            after.ports[&port]
        );
    }
}
