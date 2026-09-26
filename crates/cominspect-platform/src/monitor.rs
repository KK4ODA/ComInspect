//! Monitoring service: turns operating-system change notifications and a
//! fallback poll into debounced rescans.
//!
//! ```text
//!  OS change hint ─┐          ┌── debounce ──► scan
//!  fallback poll  ─┼─► channel┤
//!  manual refresh ─┘          └── settle rescan (after udev/driver setup)
//! ```

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use cominspect_core::model::ScanResult;

/// Message sent to the monitor thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeHint {
    /// The OS reported a change (the payload names the source).
    Os(&'static str),
    /// The user asked for a refresh.
    Refresh,
    /// Stop the monitor.
    Stop,
}

/// A running OS notification source. Dropping it stops it.
pub trait HintSource: Send {
    fn name(&self) -> &'static str;
}

/// Why a scan ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanTrigger {
    Startup,
    OsEvent,
    /// Follow-up scan shortly after an OS event, once drivers/udev have
    /// finished setting the device up (permissions, aliases, names).
    Settle,
    Poll,
    Manual,
}

#[derive(Clone, Debug)]
pub struct MonitorConfig {
    /// Full rescan interval when nothing is reported.
    pub poll_interval: Duration,
    /// Quiet period that collapses bursts of OS events into one scan.
    pub debounce: Duration,
    /// Extra scan after an OS event.
    pub settle_delay: Option<Duration>,
}

impl MonitorConfig {
    pub fn for_current_platform() -> MonitorConfig {
        let poll = if cfg!(target_os = "linux") { 15 } else { 30 };
        MonitorConfig {
            poll_interval: Duration::from_secs(poll),
            debounce: Duration::from_millis(400),
            settle_delay: Some(Duration::from_millis(1500)),
        }
    }
}

pub struct Monitor {
    tx: Sender<ChangeHint>,
    thread: Option<thread::JoinHandle<()>>,
}

type SourceFactory = Box<dyn FnOnce(Sender<ChangeHint>) -> Vec<Box<dyn HintSource>> + Send>;

impl Monitor {
    /// Starts monitoring with the platform's discovery and notification
    /// sources. `on_scan` runs on the monitor thread for every scan,
    /// starting with an initial one.
    pub fn start<F>(config: MonitorConfig, on_scan: F) -> Monitor
    where
        F: FnMut(ScanResult, ScanTrigger) + Send + 'static,
    {
        Monitor::start_with(
            config,
            crate::discover,
            Box::new(crate::os::start_sources),
            on_scan,
        )
    }

    /// Like [`Monitor::start`] with injectable discovery and sources.
    pub fn start_with<S, F>(
        config: MonitorConfig,
        scan: S,
        sources: SourceFactory,
        mut on_scan: F,
    ) -> Monitor
    where
        S: Fn() -> ScanResult + Send + 'static,
        F: FnMut(ScanResult, ScanTrigger) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<ChangeHint>();
        let source_tx = tx.clone();
        let thread = thread::Builder::new()
            .name("port-monitor".into())
            .spawn(move || {
                let active = sources(source_tx);
                log::info!(
                    "port monitor started (sources: {})",
                    if active.is_empty() {
                        "polling only".to_string()
                    } else {
                        active
                            .iter()
                            .map(|s| s.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                );
                on_scan(scan(), ScanTrigger::Startup);
                let mut settle_at: Option<Instant> = None;
                loop {
                    let timeout = match settle_at {
                        Some(at) => at.saturating_duration_since(Instant::now()),
                        None => config.poll_interval,
                    };
                    let trigger = match rx.recv_timeout(timeout) {
                        Ok(ChangeHint::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                        Ok(ChangeHint::Refresh) => ScanTrigger::Manual,
                        Ok(ChangeHint::Os(_)) => {
                            // Collapse bursts (a USB plug produces many events).
                            let deadline = Instant::now() + config.debounce;
                            let mut stop = false;
                            loop {
                                match rx.recv_timeout(
                                    deadline.saturating_duration_since(Instant::now()),
                                ) {
                                    Ok(ChangeHint::Stop) | Err(RecvTimeoutError::Disconnected) => {
                                        stop = true;
                                        break;
                                    }
                                    Ok(_) => continue,
                                    Err(RecvTimeoutError::Timeout) => break,
                                }
                            }
                            if stop {
                                break;
                            }
                            ScanTrigger::OsEvent
                        }
                        Err(RecvTimeoutError::Timeout) if settle_at.is_some() => {
                            ScanTrigger::Settle
                        }
                        Err(RecvTimeoutError::Timeout) => ScanTrigger::Poll,
                    };
                    match trigger {
                        ScanTrigger::OsEvent => {
                            settle_at = config.settle_delay.map(|d| Instant::now() + d);
                        }
                        ScanTrigger::Settle => settle_at = None,
                        _ => {}
                    }
                    on_scan(scan(), trigger);
                }
                drop(active);
                log::info!("port monitor stopped");
            })
            .expect("spawn monitor thread");
        Monitor {
            tx,
            thread: Some(thread),
        }
    }

    /// Requests an immediate rescan.
    pub fn refresh(&self) {
        let _ = self.tx.send(ChangeHint::Refresh);
    }

    /// Sender that can be used to inject hints (e.g. from other components).
    pub fn sender(&self) -> Sender<ChangeHint> {
        self.tx.clone()
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.tx.send(ChangeHint::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn fake_scan() -> ScanResult {
        ScanResult {
            platform: "test".into(),
            ..Default::default()
        }
    }

    #[test]
    fn debounces_bursts_and_settles() {
        let triggers: Arc<Mutex<Vec<ScanTrigger>>> = Arc::default();
        let seen = triggers.clone();
        let hint_tx: Arc<Mutex<Option<Sender<ChangeHint>>>> = Arc::default();
        let capture = hint_tx.clone();
        let monitor = Monitor::start_with(
            MonitorConfig {
                poll_interval: Duration::from_secs(60),
                debounce: Duration::from_millis(100),
                settle_delay: Some(Duration::from_millis(200)),
            },
            fake_scan,
            Box::new(move |tx| {
                *capture.lock().unwrap() = Some(tx);
                Vec::new()
            }),
            move |_, trigger| seen.lock().unwrap().push(trigger),
        );
        // Wait for the startup scan.
        let start = Instant::now();
        while triggers.lock().unwrap().is_empty() && start.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(10));
        }
        let tx = hint_tx.lock().unwrap().clone().unwrap();
        for _ in 0..20 {
            tx.send(ChangeHint::Os("test")).unwrap();
        }
        thread::sleep(Duration::from_millis(600));
        monitor.refresh();
        thread::sleep(Duration::from_millis(100));
        drop(monitor);
        let got = triggers.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![
                ScanTrigger::Startup,
                ScanTrigger::OsEvent,
                ScanTrigger::Settle,
                ScanTrigger::Manual
            ]
        );
    }
}
