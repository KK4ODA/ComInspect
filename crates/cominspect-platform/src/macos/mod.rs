//! macOS adapter.
//!
//! * `interpret` — pure interpretation (compiled and tested everywhere).
//! * `iokit` — IOKit collection (macOS only).
//! * Monitoring polls the `/dev/cu.*` name list, which is cheap, and triggers
//!   a full IOKit scan when it changes.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

pub mod interpret;

#[cfg(target_os = "macos")]
mod iokit;

pub use interpret::capabilities;

#[cfg(target_os = "macos")]
pub use iokit::discover;

#[cfg(target_os = "macos")]
pub(crate) use poll::start_sources;

#[cfg(target_os = "macos")]
mod poll {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::Sender;
    use std::thread;
    use std::time::Duration;

    use crate::monitor::{ChangeHint, HintSource};

    fn callout_names() -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir("/dev")
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.starts_with("cu."))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    struct DevPoll {
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl HintSource for DevPoll {
        fn name(&self) -> &'static str {
            "dev-poll"
        }
    }

    impl Drop for DevPoll {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    pub(crate) fn start_sources(tx: Sender<ChangeHint>) -> Vec<Box<dyn HintSource>> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::Builder::new()
            .name("dev-poll".into())
            .spawn(move || {
                let mut last = callout_names();
                while !flag.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(1500));
                    let now = callout_names();
                    if now != last {
                        last = now;
                        if tx.send(ChangeHint::Os("dev")).is_err() {
                            break;
                        }
                    }
                }
            });
        match thread {
            Ok(t) => vec![Box::new(DevPoll {
                stop,
                thread: Some(t),
            })],
            Err(_) => Vec::new(),
        }
    }
}
