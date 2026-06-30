//! `agent-collector` — data sources behind the [`Collector`](agent_core::Collector) trait.
//!
//! The `mock` source is permanent and needs no GPU (see ARCHITECTURE.md): it keeps every build/test/demo path
//! working with no driver present. The NVML source (the source of truth), DCGM, and the inference
//! scraper land in Phase 1/Phase 6.

#![forbid(unsafe_code)]

use std::time::SystemTime;

use agent_core::{Bytes, CollectError, Collector, DeviceSample, Pct};
use nvml_wrapper::Nvml;

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

/// The NVIDIA source of truth — NVML via dlopen (no driver needed at build time). Construction loads
/// NVML and fails cleanly on a host with no GPU/driver, so the caller can fall back to the mock.
pub struct NvmlCollector {
    // `Device` handles borrow this, so we fetch them per-sample rather than storing them (no
    // self-referential struct). `Nvml` runs `nvmlShutdown` on drop — no manual teardown.
    nvml: Nvml,
}

impl NvmlCollector {
    /// Initialise NVML. Returns `Err` on a host with no NVIDIA driver/GPU.
    ///
    /// # Errors
    /// If NVML cannot be loaded or initialised (e.g. no driver present).
    pub fn new() -> Result<Self, CollectError> {
        let nvml =
            Nvml::init().map_err(|e| CollectError::Source(format!("NVML init failed: {e}")))?;
        Ok(Self { nvml })
    }
}

impl Collector for NvmlCollector {
    fn name(&self) -> &str {
        "nvml"
    }

    fn sample(&mut self) -> Result<Vec<DeviceSample>, CollectError> {
        // TODO(§0.5 R3): the engine stamps `ts` once the sampling loop lands; the collector stops stamping.
        let ts = SystemTime::now();
        let count = self.nvml.device_count().map_err(nvml_err)?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let dev = self.nvml.device_by_index(i).map_err(nvml_err)?;
            let util = dev.utilization_rates().map_err(nvml_err)?;
            let mem = dev.memory_info().map_err(nvml_err)?;
            let name = dev.name().map_err(nvml_err)?;
            out.push(sample_from(i, name, util.gpu, mem.used, mem.total, ts));
        }
        Ok(out)
    }
}

/// Map an NVML error to the vendor-neutral [`CollectError`] at the boundary, so `core` never depends on
/// NVIDIA. (Per-device signal states — asleep/unsupported — arrive with Phase 2.)
fn nvml_err(e: nvml_wrapper::error::NvmlError) -> CollectError {
    CollectError::Source(format!("NVML: {e}"))
}

/// Pure NVML-readings → [`DeviceSample`] mapping, split out so the unit conversions are testable with no GPU.
fn sample_from(
    index: u32,
    name: String,
    util_gpu: u32,
    mem_used: u64,
    mem_total: u64,
    ts: SystemTime,
) -> DeviceSample {
    // `util_gpu` is a percent; clamp into a `u8` without a truncating `as` cast.
    let gpu_pct = u8::try_from(util_gpu.min(100)).unwrap_or(100);
    DeviceSample::new(
        index,
        name,
        Pct::clamped(gpu_pct),
        Bytes(mem_used),
        Bytes(mem_total),
        ts,
    )
}

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

    #[test]
    fn sample_from_maps_and_clamps_nvml_readings() {
        // An over-100 utilization (a source can overshoot) clamps to 100; bytes pass through; name kept.
        let s = sample_from(
            1,
            "NVIDIA H100".into(),
            250,
            8 << 30,
            80 << 30,
            SystemTime::now(),
        );
        assert_eq!(s.index, 1);
        assert_eq!(s.name, "NVIDIA H100");
        assert_eq!(s.util.get(), 100);
        assert_eq!(s.mem_used.get(), 8 << 30);
        assert_eq!(s.mem_total.get(), 80 << 30);

        // A normal reading is preserved exactly.
        let s = sample_from(0, "g".into(), 50, 0, 0, SystemTime::now());
        assert_eq!(s.util.get(), 50);
    }
}
