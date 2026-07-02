//! `agent-collector` — data sources behind the [`Collector`](agent_core::Collector) trait.
//!
//! The `mock` source is permanent and needs no GPU (see ARCHITECTURE.md): it keeps every build/test/demo
//! path working with no driver present. `NvmlCollector` is the NVIDIA source of truth; DCGM (richer
//! counters, real SM occupancy) and the inference scraper land later.

#![forbid(unsafe_code)]

use std::time::SystemTime;

use agent_core::{
    Bytes, Celsius, CollectError, Collector, DeviceSample, KbPerSec, Megahertz, Metrics,
    Milliwatts, Pct,
};
use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::{Clock, PcieUtilCounter, TemperatureSensor};

/// A synthetic source: a slow triangle wave per device, so the app builds, tests, and demos with no GPU.
/// Select it with `--mock` (or `AGENT_SOURCE=mock`). Permanent and first-class — it's the "ideal
/// fully-featured device", so it populates *every* metric (a real GPU legitimately leaves some `None`).
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
                let u = u32::from(util);
                // Plausible values that move with util, so every surface shows live variation.
                let metrics = Metrics::new(
                    Pct::clamped(util),
                    Bytes(4_000_000_000 + u64::from(util) * 50_000_000),
                    Bytes(24_000_000_000),
                )
                .with_temperature(Some(Celsius(30 + u / 2))) // 30..80 °C
                .with_power(Some(Milliwatts(50_000 + u * 2_500))) // 50..300 W
                .with_clocks(Some(Megahertz(300 + u * 15)), Some(Megahertz(5_001)))
                .with_sm_occupancy(Some(Pct::clamped(util.saturating_sub(5))))
                .with_pcie(Some(KbPerSec(u * 1_000)), Some(KbPerSec(u * 800)))
                .with_fan(Some(Pct::clamped(30 + util / 2)));
                DeviceSample::new(i, format!("Mock GPU {i}"), metrics, now)
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
        let ts = SystemTime::now();
        let count = self.nvml.device_count().map_err(nvml_err)?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let dev = self.nvml.device_by_index(i).map_err(nvml_err)?;
            // util + memory are mandatory (a device that can't report these is a hard error); every other
            // metric is best-effort — `.ok()` turns a per-metric `NotSupported` into `None` so one absent
            // reading (e.g. fan on a fanless datacenter card) never fails the whole sample.
            let util = dev.utilization_rates().map_err(nvml_err)?;
            let mem = dev.memory_info().map_err(nvml_err)?;
            let name = dev.name().map_err(nvml_err)?;
            let metrics = metrics_from(
                util.gpu,
                mem.used,
                mem.total,
                dev.temperature(TemperatureSensor::Gpu).ok(),
                dev.power_usage().ok(),
                dev.clock_info(Clock::SM).ok(),
                dev.clock_info(Clock::Memory).ok(),
                dev.pcie_throughput(PcieUtilCounter::Send).ok(),
                dev.pcie_throughput(PcieUtilCounter::Receive).ok(),
                dev.fan_speed(0).ok(),
            );
            out.push(DeviceSample::new(i, name, metrics, ts));
        }
        Ok(out)
    }
}

/// Map an NVML error to the vendor-neutral [`CollectError`] at the boundary, so `core` never depends on
/// NVIDIA.
fn nvml_err(e: nvml_wrapper::error::NvmlError) -> CollectError {
    CollectError::Source(format!("NVML: {e}"))
}

/// Clamp a raw percent (which a source can overshoot) into a [`Pct`], without a truncating `as` cast.
fn clamp_pct(v: u32) -> Pct {
    Pct::clamped(u8::try_from(v.min(100)).unwrap_or(100))
}

/// Pure NVML-readings → [`Metrics`] mapping (unit wrap + clamp), split out so it's testable with no GPU.
/// SM occupancy isn't produced here — base NVML doesn't expose it (it comes from DCGM).
#[expect(
    clippy::too_many_arguments,
    reason = "a flat 1:1 mapper over NVML's independent per-metric reads; a struct would add ceremony without clarity"
)]
fn metrics_from(
    util_gpu: u32,
    mem_used: u64,
    mem_total: u64,
    temperature_c: Option<u32>,
    power_mw: Option<u32>,
    sm_clock_mhz: Option<u32>,
    mem_clock_mhz: Option<u32>,
    pcie_tx_kb_s: Option<u32>,
    pcie_rx_kb_s: Option<u32>,
    fan_pct: Option<u32>,
) -> Metrics {
    Metrics::new(clamp_pct(util_gpu), Bytes(mem_used), Bytes(mem_total))
        .with_temperature(temperature_c.map(Celsius))
        .with_power(power_mw.map(Milliwatts))
        .with_clocks(sm_clock_mhz.map(Megahertz), mem_clock_mhz.map(Megahertz))
        .with_pcie(pcie_tx_kb_s.map(KbPerSec), pcie_rx_kb_s.map(KbPerSec))
        .with_fan(fan_pct.map(clamp_pct))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_yields_fully_featured_devices_without_a_gpu() {
        let mut c = MockCollector::new(2);
        let samples = c.sample().unwrap_or_default();
        assert_eq!(samples.len(), 2);
        assert_eq!(c.name(), "mock");
        assert!(samples.iter().all(|s| s.metrics.util.get() <= 100));
        // the mock is the "ideal" source — every optional metric is populated
        assert!(samples.iter().all(|s| {
            let m = &s.metrics;
            m.temperature.is_some()
                && m.power.is_some()
                && m.sm_clock.is_some()
                && m.mem_clock.is_some()
                && m.sm_occupancy.is_some()
                && m.pcie_tx.is_some()
                && m.pcie_rx.is_some()
                && m.fan.is_some()
        }));
    }

    #[test]
    fn metrics_from_wraps_clamps_and_passes_none_through() {
        let m = metrics_from(
            250, // util overshoots -> clamps to 100
            8 << 30,
            80 << 30,
            Some(61),      // temperature °C
            Some(150_000), // power mW
            Some(1_800),   // sm clock MHz
            Some(9_500),   // mem clock MHz
            Some(2_000),   // pcie tx KB/s
            None,          // pcie rx unsupported
            Some(120),     // fan overshoots -> clamps to 100
        );
        assert_eq!(m.util.get(), 100);
        assert_eq!((m.mem_used.get(), m.mem_total.get()), (8 << 30, 80 << 30));
        assert_eq!(m.temperature, Some(Celsius(61)));
        assert_eq!(m.power, Some(Milliwatts(150_000)));
        assert_eq!(m.sm_clock, Some(Megahertz(1_800)));
        assert_eq!(m.mem_clock, Some(Megahertz(9_500)));
        assert_eq!(m.pcie_tx, Some(KbPerSec(2_000)));
        assert_eq!(m.pcie_rx, None);
        assert_eq!(m.fan, Some(Pct::clamped(100)));
        assert_eq!(m.sm_occupancy, None); // never from NVML
    }
}
