//! Operating systems without a usage check.

use super::{PortUsage, UsageSnapshot};

pub struct Probe;

impl Probe {
    pub fn new() -> Probe {
        Probe
    }

    pub fn check(&mut self, ports: &[String]) -> UsageSnapshot {
        UsageSnapshot {
            ports: ports
                .iter()
                .map(|p| {
                    (
                        p.clone(),
                        PortUsage::Unknown {
                            reason: "not supported on this operating system".into(),
                        },
                    )
                })
                .collect(),
            limitation: None,
            duration_ms: 0,
        }
    }
}
