//! Linux: processes whose `/proc/<pid>/fd` links point at the port's device
//! node. Reading those links never touches the device.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use super::{PortHolder, PortUsage, UsageSnapshot};

pub struct Probe;

impl Probe {
    pub fn new() -> Probe {
        Probe
    }

    pub fn check(&mut self, ports: &[String]) -> UsageSnapshot {
        let mut result = BTreeMap::new();
        // Device node -> requested port names (a by-id alias and the node
        // itself resolve to the same device).
        let mut targets: HashMap<PathBuf, Vec<String>> = HashMap::new();
        for port in ports {
            match fs::canonicalize(port) {
                Ok(path) => targets.entry(path).or_default().push(port.clone()),
                Err(_) => {
                    result.insert(
                        port.clone(),
                        PortUsage::Unknown {
                            reason: "the port does not exist".into(),
                        },
                    );
                }
            }
        }

        let mut holders: HashMap<&str, Vec<PortHolder>> = HashMap::new();
        if !targets.is_empty()
            && let Ok(procs) = fs::read_dir("/proc")
        {
            for entry in procs.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<u32>().ok())
                else {
                    continue;
                };
                // Unreadable for other users' processes; that is the
                // documented limitation.
                let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
                    continue;
                };
                let mut holder: Option<PortHolder> = None;
                for fd in fds.flatten() {
                    let Ok(link) = fs::read_link(fd.path()) else {
                        continue;
                    };
                    let Some(names) = targets.get(&link) else {
                        continue;
                    };
                    let holder = holder.get_or_insert_with(|| describe(pid, &entry.path()));
                    for name in names {
                        let list = holders.entry(name.as_str()).or_default();
                        if !list.iter().any(|h| h.pid == pid) {
                            list.push(holder.clone());
                        }
                    }
                }
            }
        }

        for names in targets.values() {
            for name in names {
                let found = holders.remove(name.as_str()).unwrap_or_default();
                result.insert(name.clone(), PortUsage::in_use(found));
            }
        }

        // SAFETY: geteuid has no preconditions.
        let root = unsafe { libc::geteuid() } == 0;
        UsageSnapshot {
            ports: result,
            limitation: (!root).then(|| {
                "Programs run by other users or as root are not visible unless ComInspect runs \
                 as root."
                    .into()
            }),
            duration_ms: 0,
        }
    }
}

fn describe(pid: u32, proc_dir: &Path) -> PortHolder {
    let executable = fs::read_link(proc_dir.join("exe")).ok();
    let comm = fs::read_to_string(proc_dir.join("comm"))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty());
    let process_name = executable
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .or(comm)
        .unwrap_or_else(|| format!("process {pid}"));
    PortHolder {
        pid,
        process_name,
        executable: executable.map(|p| p.to_string_lossy().into_owned()),
        description: None,
    }
}
