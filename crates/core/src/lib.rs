//! `agent-core` — the data model and the source of truth for the monitor.
//!
//! Device/process snapshots, bounded time series, and the [`Collector`] trait every data source
//! implements. **Every frontend (GUI/TUI/CLI) renders this** and nothing else — see ARCHITECTURE.md
//! (headless engine, pure-view frontends).

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::time::SystemTime;

/// Strongly-typed units — no bare numerics cross the model boundary (see `.rules`).
/// `Celsius`/`Watts` arrive in Phase 2 with temperature/power.
pub mod units {
    /// GPU utilization as a whole percent, `0..=100`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Pct(pub u8);

    impl Pct {
        /// Wrap a raw percent, clamping into `0..=100` (a source can overshoot).
        #[must_use]
        pub fn clamped(v: u8) -> Self {
            Self(v.min(100))
        }
        /// The raw percent.
        #[must_use]
        pub fn get(self) -> u8 {
            self.0
        }
    }

    /// A byte count (memory). Lossless and source-native — NVML reports bytes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Bytes(pub u64);

    impl Bytes {
        /// The raw byte count.
        #[must_use]
        pub fn get(self) -> u64 {
            self.0
        }
        /// Mebibytes, for display only (`1 MiB = 1 << 20` bytes); truncating.
        #[must_use]
        pub fn as_mib(self) -> u64 {
            self.0 >> 20
        }
    }
}

pub use units::{Bytes, Pct};

/// A single point-in-time sample of one GPU device. `#[non_exhaustive]` so fields can be added
/// (temperature, power, …) without breaking renderers — construct via [`DeviceSample::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeviceSample {
    /// Stable device index (0-based).
    pub index: u32,
    /// Human-readable name (e.g. "NVIDIA H100").
    pub name: String,
    /// GPU utilization.
    pub util: Pct,
    /// Memory used.
    pub mem_used: Bytes,
    /// Total memory.
    pub mem_total: Bytes,
    // TODO(§0.5 R3): the engine stamps `ts` once the sampling loop lands; the collector stops stamping.
    /// When the sample was taken.
    pub ts: SystemTime,
}

impl DeviceSample {
    /// Build a sample. Required because `#[non_exhaustive]` forbids struct-literal construction
    /// from other crates (e.g. `agent-collector`).
    #[must_use]
    pub fn new(
        index: u32,
        name: String,
        util: Pct,
        mem_used: Bytes,
        mem_total: Bytes,
        ts: SystemTime,
    ) -> Self {
        Self {
            index,
            name,
            util,
            mem_used,
            mem_total,
            ts,
        }
    }
}

/// A bounded ring-buffer time series (oldest at the front, newest at the back). Never grows past
/// `cap` — a monitor's memory must not scale with uptime.
#[derive(Debug)]
pub struct Series<T> {
    buf: VecDeque<T>,
    cap: usize,
}

impl<T> Series<T> {
    /// A new series holding at most `cap` points.
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap),
            cap,
        }
    }

    /// Push a point, dropping the oldest if at capacity.
    pub fn push(&mut self, v: T) {
        if self.cap == 0 {
            return;
        }
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(v);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Oldest-to-newest iterator over the retained points.
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.buf.iter()
    }

    /// The most recent point, if any.
    #[must_use]
    pub fn latest(&self) -> Option<&T> {
        self.buf.back()
    }
}

/// One retained reading in a device's series — `Copy`, so the per-slot history holds no `String`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Point {
    /// When the reading was taken.
    pub ts: SystemTime,
    /// GPU utilization.
    pub util: Pct,
    /// Memory used.
    pub mem_used: Bytes,
    /// Total memory.
    pub mem_total: Bytes,
}

/// Bounded history for a single device: stable identity plus one ring-buffer of [`Point`]s.
#[derive(Debug)]
pub struct DeviceHistory {
    /// Stable device index.
    pub index: u32,
    /// Human-readable name (captured once, when the device is first seen).
    pub name: String,
    series: Series<Point>,
}

impl DeviceHistory {
    /// A new, empty history for `index`/`name`, holding at most `cap` points.
    #[must_use]
    pub fn new(index: u32, name: String, cap: usize) -> Self {
        Self {
            index,
            name,
            series: Series::new(cap),
        }
    }

    /// The retained points, oldest-to-newest.
    #[must_use]
    pub fn series(&self) -> &Series<Point> {
        &self.series
    }

    /// The most recent point, if any.
    #[must_use]
    pub fn latest(&self) -> Option<&Point> {
        self.series.latest()
    }

    /// Number of retained points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.series.len()
    }

    /// Whether any points are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }
}

/// The bounded, per-device history the surfaces render. One ring-buffer per device, matched by
/// index; memory is bounded at `devices × cap` and never grows with uptime.
#[derive(Debug)]
pub struct Model {
    cap: usize,
    devices: Vec<DeviceHistory>,
}

impl Model {
    /// A new model giving each device a `cap`-point history (`cap = history_window / sample_interval`,
    /// chosen by the engine).
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            devices: Vec::new(),
        }
    }

    /// Append each sample to its device's series, creating the per-device history on first sight.
    /// Stores whatever `ts` the sample carries — it neither generates nor overrides timestamps.
    pub fn ingest(&mut self, samples: &[DeviceSample]) {
        for s in samples {
            let p = Point {
                ts: s.ts,
                util: s.util,
                mem_used: s.mem_used,
                mem_total: s.mem_total,
            };
            match self.devices.iter_mut().find(|d| d.index == s.index) {
                Some(d) => d.series.push(p),
                None => {
                    let mut d = DeviceHistory::new(s.index, s.name.clone(), self.cap);
                    d.series.push(p);
                    self.devices.push(d);
                }
            }
        }
    }

    /// All device histories, in first-seen order.
    #[must_use]
    pub fn devices(&self) -> &[DeviceHistory] {
        &self.devices
    }

    /// The history for a specific device index, if seen.
    #[must_use]
    pub fn device(&self, index: u32) -> Option<&DeviceHistory> {
        self.devices.iter().find(|d| d.index == index)
    }

    /// Number of devices tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.devices.len()
    }

    /// Whether any device is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

/// A data source: NVML, DCGM, the inference scraper, or the mock. Frontends never call this — only
/// the collector loop does (see ARCHITECTURE.md). Errors degrade to a "no signal" state, never a panic.
pub trait Collector {
    /// A short label for the source ("nvml", "mock", …).
    fn name(&self) -> &str;
    /// Sample every visible device once.
    fn sample(&mut self) -> Result<Vec<DeviceSample>, CollectError>;
}

/// A recoverable failure to sample a source (driver missing, device asleep, …).
#[derive(Debug)]
pub struct CollectError(pub String);

impl std::fmt::Display for CollectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "collector error: {}", self.0)
    }
}

impl std::error::Error for CollectError {}

/// A machine-facing view of the model — a Prometheus `/metrics` endpoint, an OTLP exporter, a Splunk
/// sink, a JSON stream. The mirror of [`Collector`]: data flows *out*. Like the human frontends, a
/// sink only reads the model; it never reaches into a data source ("integrate, don't reimplement").
/// Stubbed here as the export seam; implementations land in Phase 9.
pub trait Sink {
    /// A short label for the sink ("prometheus", "otlp", …).
    fn name(&self) -> &str;
    /// Publish the current device snapshots downstream. A failure must degrade quietly — a broken sink
    /// never disrupts the engine or a frontend.
    fn publish(&mut self, devices: &[DeviceSample]) -> Result<(), SinkError>;
}

/// A recoverable failure to publish to a sink (endpoint unreachable, encode error, …).
#[derive(Debug)]
pub struct SinkError(pub String);

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sink error: {}", self.0)
    }
}

impl std::error::Error for SinkError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_is_bounded_and_ordered() {
        let mut s = Series::new(3);
        for i in 0..5 {
            s.push(i);
        }
        assert_eq!(s.len(), 3);
        assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
        assert_eq!(s.latest(), Some(&4));
    }

    #[test]
    fn zero_capacity_series_stays_empty() {
        let mut s = Series::new(0);
        s.push(1);
        assert!(s.is_empty());
    }

    fn sample(index: u32, util: u8) -> DeviceSample {
        DeviceSample::new(
            index,
            format!("GPU {index}"),
            Pct::clamped(util),
            Bytes(1 << 30),
            Bytes(24 << 30),
            SystemTime::now(),
        )
    }

    #[test]
    fn pct_clamps_and_orders() {
        assert_eq!(Pct::clamped(250), Pct(100));
        assert_eq!(Pct::clamped(42).get(), 42);
        assert!(Pct(10) < Pct(90));
    }

    #[test]
    fn bytes_round_trips_and_converts() {
        assert_eq!(Bytes(2 << 20).as_mib(), 2);
        assert_eq!(Bytes(4_000_000_000).get(), 4_000_000_000);
    }

    #[test]
    fn model_ingest_bounds_and_orders_per_device() {
        let mut m = Model::new(3);
        for u in 0..5u8 {
            m.ingest(&[sample(0, u)]);
        }
        let d = m.device(0).expect("device 0 present");
        assert_eq!(d.len(), 3); // bounded: oldest dropped
        let utils: Vec<u8> = d.series().iter().map(|p| p.util.get()).collect();
        assert_eq!(utils, vec![2, 3, 4]); // ordered oldest→newest
        assert_eq!(d.latest().map(|p| p.util.get()), Some(4));
    }

    #[test]
    fn model_tracks_devices_independently() {
        let mut m = Model::new(8);
        m.ingest(&[sample(0, 10), sample(1, 20)]);
        m.ingest(&[sample(0, 11), sample(1, 21)]);
        assert_eq!(m.len(), 2);
        assert_eq!(m.device(0).map(DeviceHistory::len), Some(2));
        assert_eq!(
            m.device(1)
                .and_then(DeviceHistory::latest)
                .map(|p| p.util.get()),
            Some(21)
        );
        assert!(m.device(2).is_none());
    }

    #[test]
    fn core_types_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Model>();
        assert_send_sync::<DeviceSample>();
        assert_send_sync::<DeviceHistory>();
        assert_send_sync::<Point>();
        assert_send_sync::<Series<Point>>();
    }
}
