//! `agent-collector` — data sources behind the [`Collector`](agent_core::Collector) trait.
//!
//! The `mock` source is permanent and needs no GPU (see ARCHITECTURE.md): it keeps every build/test/demo path
//! working with no driver present. The NVML source (the source of truth), DCGM, and the inference
//! scraper land in Phase 1/Phase 6.

use std::time::SystemTime;

use agent_core::{Bytes, CollectError, Collector, DeviceSample, Pct};

/// A synthetic source: a slow triangle wave per device, so the app builds, tests, and demos with no
/// GPU. Select it with `--mock` (or `AGENT_SOURCE=mock`). Permanent and first-class.
pub struct MockCollector {
    tick: u64,
    devices: u32,
}

impl MockCollector {
    /// A mock reporting `devices` synthetic GPUs.
    #[must_use]
    pub fn new(devices: u32) -> Self {
        Self { tick: 0, devices }
    }
}

impl Default for MockCollector {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Collector for MockCollector {
    fn name(&self) -> &str {
        "mock"
    }

    fn sample(&mut self) -> Result<Vec<DeviceSample>, CollectError> {
        self.tick = self.tick.wrapping_add(1);
        let now = SystemTime::now();
        let out = (0..self.devices)
            .map(|i| {
                // A triangle wave in 0..=100, offset per device — no float deps.
                let phase = self.tick.wrapping_add(u64::from(i) * 17) % 100;
                let base = if phase < 50 { phase } else { 100 - phase }; // 0..=50
                let util = (base as u8).saturating_mul(2); // 0..=100
                DeviceSample::new(
                    i,
                    format!("Mock GPU {i}"),
                    Pct::clamped(util),
                    Bytes(4_000_000_000 + u64::from(util) * 50_000_000),
                    Bytes(24_000_000_000),
                    now,
                )
            })
            .collect();
        Ok(out)
    }
}

// TODO(Phase 1): `NvmlCollector` via `nvml-wrapper` — NVML is the source of truth (see ARCHITECTURE.md).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_yields_devices_without_a_gpu() {
        let mut c = MockCollector::new(2);
        let samples = c.sample().unwrap_or_default();
        assert_eq!(samples.len(), 2);
        assert!(samples.iter().all(|s| s.util.get() <= 100));
        assert_eq!(c.name(), "mock");
    }
}
