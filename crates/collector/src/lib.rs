//! `gpumon-collector` — data sources behind the [`Collector`](gpumon_core::Collector) trait.
//!
//! The `mock` source is permanent and needs no GPU (see ARCHITECTURE.md): it keeps every build/test/demo path
//! working with no driver present. The NVML source (the source of truth), DCGM, and the inference
//! scraper land in M1/M6.

use std::time::SystemTime;

use gpumon_core::{CollectError, Collector, DeviceSample};

/// A synthetic source: a slow triangle wave per device, so the app builds, tests, and demos with no
/// GPU. Select it with `--mock` (or `GPUMON_SOURCE=mock`). Permanent and first-class.
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
                DeviceSample {
                    index: i,
                    name: format!("Mock GPU {i}"),
                    util_pct: util,
                    mem_used: 4_000_000_000 + u64::from(util) * 50_000_000,
                    mem_total: 24_000_000_000,
                    ts: now,
                }
            })
            .collect();
        Ok(out)
    }
}

// TODO(M1): `NvmlCollector` via `nvml-wrapper` — NVML is the source of truth (see ARCHITECTURE.md).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_yields_devices_without_a_gpu() {
        let mut c = MockCollector::new(2);
        let samples = c.sample().unwrap_or_default();
        assert_eq!(samples.len(), 2);
        assert!(samples.iter().all(|s| s.util_pct <= 100));
        assert_eq!(c.name(), "mock");
    }
}
