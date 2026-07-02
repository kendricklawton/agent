//! `agent-core` — the data model and the source of truth for the monitor.
//!
//! Device/process snapshots, bounded time series, and the [`Collector`] trait every data source
//! implements. **Every frontend (GUI/TUI/CLI) renders this** and nothing else — see ARCHITECTURE.md
//! (headless engine, pure-view frontends).

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::time::SystemTime;

/// Strongly-typed units — no bare numerics cross the model boundary (see `.rules`). Each wraps the
/// lossless, source-native unit NVML reports; display helpers convert.
pub mod units {
    /// GPU utilization / fan speed / occupancy as a whole percent, `0..=100`.
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

    /// A temperature in whole degrees Celsius (NVML reports °C).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Celsius(pub u32);

    impl Celsius {
        /// The raw degrees Celsius.
        #[must_use]
        pub fn get(self) -> u32 {
            self.0
        }
    }

    /// Power in milliwatts (lossless, source-native — NVML reports mW).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Milliwatts(pub u32);

    impl Milliwatts {
        /// The raw milliwatts.
        #[must_use]
        pub fn get(self) -> u32 {
            self.0
        }
        /// Watts, for display only.
        #[must_use]
        pub fn as_watts(self) -> f64 {
            f64::from(self.0) / 1000.0
        }
    }

    /// A clock frequency in megahertz (NVML reports MHz).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Megahertz(pub u32);

    impl Megahertz {
        /// The raw megahertz.
        #[must_use]
        pub fn get(self) -> u32 {
            self.0
        }
    }

    /// A throughput in kilobytes per second (NVML reports PCIe utilization in KB/s).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct KbPerSec(pub u32);

    impl KbPerSec {
        /// The raw KB/s.
        #[must_use]
        pub fn get(self) -> u32 {
            self.0
        }
    }
}

pub use units::{Bytes, Celsius, KbPerSec, Megahertz, Milliwatts, Pct};

pub use engine::{DeviceSnapshot, SignalState, Snapshot};

/// The metrics sampled for one device at one instant. `util`/memory are always present (the Phase 1
/// baseline); the rest are `Option` because not every source/GPU exposes every metric — a fanless
/// datacenter card has no fan, and SM occupancy needs DCGM (base NVML doesn't report it).
/// `#[non_exhaustive]` + the `with_*` chain keep it additive: a new metric is one field here, shared by
/// both [`DeviceSample`] and [`Point`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Metrics {
    /// GPU utilization.
    pub util: Pct,
    /// Memory used.
    pub mem_used: Bytes,
    /// Total memory.
    pub mem_total: Bytes,
    /// GPU die temperature.
    pub temperature: Option<Celsius>,
    /// Board power draw.
    pub power: Option<Milliwatts>,
    /// SM (graphics) clock.
    pub sm_clock: Option<Megahertz>,
    /// Memory clock.
    pub mem_clock: Option<Megahertz>,
    /// SM occupancy — from DCGM; `None` on base NVML.
    pub sm_occupancy: Option<Pct>,
    /// PCIe transmit throughput.
    pub pcie_tx: Option<KbPerSec>,
    /// PCIe receive throughput.
    pub pcie_rx: Option<KbPerSec>,
    /// Fan speed.
    pub fan: Option<Pct>,
}

impl Metrics {
    /// The always-present metrics; every optional starts `None`. Fill optionals with the `with_*` chain.
    #[must_use]
    pub fn new(util: Pct, mem_used: Bytes, mem_total: Bytes) -> Self {
        Self {
            util,
            mem_used,
            mem_total,
            temperature: None,
            power: None,
            sm_clock: None,
            mem_clock: None,
            sm_occupancy: None,
            pcie_tx: None,
            pcie_rx: None,
            fan: None,
        }
    }

    /// Set the die temperature.
    #[must_use]
    pub fn with_temperature(mut self, v: Option<Celsius>) -> Self {
        self.temperature = v;
        self
    }
    /// Set the board power draw.
    #[must_use]
    pub fn with_power(mut self, v: Option<Milliwatts>) -> Self {
        self.power = v;
        self
    }
    /// Set the SM and memory clocks.
    #[must_use]
    pub fn with_clocks(mut self, sm: Option<Megahertz>, mem: Option<Megahertz>) -> Self {
        self.sm_clock = sm;
        self.mem_clock = mem;
        self
    }
    /// Set SM occupancy.
    #[must_use]
    pub fn with_sm_occupancy(mut self, v: Option<Pct>) -> Self {
        self.sm_occupancy = v;
        self
    }
    /// Set PCIe transmit/receive throughput.
    #[must_use]
    pub fn with_pcie(mut self, tx: Option<KbPerSec>, rx: Option<KbPerSec>) -> Self {
        self.pcie_tx = tx;
        self.pcie_rx = rx;
        self
    }
    /// Set fan speed.
    #[must_use]
    pub fn with_fan(mut self, v: Option<Pct>) -> Self {
        self.fan = v;
        self
    }
}

/// A single point-in-time sample of one GPU device: stable identity, its [`Metrics`], and a timestamp.
/// `#[non_exhaustive]` — construct via [`DeviceSample::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeviceSample {
    /// Stable device index (0-based).
    pub index: u32,
    /// Human-readable name (e.g. "NVIDIA H100").
    pub name: String,
    /// The sampled metrics.
    pub metrics: Metrics,
    /// When the sample was taken. The engine restamps this with its own clock on ingest (one clock —
    /// §0.5 R3), so a collector's own value is overwritten.
    pub ts: SystemTime,
}

impl DeviceSample {
    /// Build a sample. Required because `#[non_exhaustive]` forbids struct-literal construction
    /// from other crates (e.g. `agent-collector`).
    #[must_use]
    pub fn new(index: u32, name: String, metrics: Metrics, ts: SystemTime) -> Self {
        Self {
            index,
            name,
            metrics,
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
    /// The sampled metrics.
    pub metrics: Metrics,
}

impl Point {
    /// Build a point. Needed because `#[non_exhaustive]` forbids struct-literal construction from other
    /// crates (e.g. a surface building view/test data).
    #[must_use]
    pub fn new(ts: SystemTime, metrics: Metrics) -> Self {
        Self { ts, metrics }
    }
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
                metrics: s.metrics,
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
/// the engine loop does (see ARCHITECTURE.md). Errors degrade to a "no signal" state, never a panic.
///
/// `Send`, because the engine owns the collector on its background sampling thread (§0.5 R10).
pub trait Collector: Send {
    /// A short label for the source ("nvml", "mock", …).
    fn name(&self) -> &str;
    /// Sample every visible device once.
    fn sample(&mut self) -> Result<Vec<DeviceSample>, CollectError>;
}

/// A recoverable failure to sample a source (driver missing, device asleep, …).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CollectError {
    /// The data source could not be sampled.
    #[error("collector error: {0}")]
    Source(String),
}

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
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SinkError {
    /// The sink could not publish the snapshot.
    #[error("sink error: {0}")]
    Publish(String),
}

/// The headless engine: it owns the one sampling loop, stamps the clock, appends to a [`Model`], and
/// publishes an immutable [`Snapshot`] that every surface reads lock-free (ARCHITECTURE.md §0.5, R1/R10).
pub mod engine {
    use std::sync::Arc;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::thread::JoinHandle;
    use std::time::{Duration, SystemTime};

    use arc_swap::ArcSwap;

    use super::{Collector, DeviceSample, Model, Point};

    /// Hard floor on the sample interval — the engine never samples faster than this, so no config can
    /// make the loop busy-spin or the monitor a hog (keystone 6).
    const MIN_INTERVAL: Duration = Duration::from_millis(10);

    /// How a signal renders when it isn't a clean reading — every surface renders this explicitly, never
    /// a blank or a panic (§0.5). Phase 1 produces `Ok`/`NoData`/`Stale`; per-device states (Phase 2) and
    /// remote hosts (Phase 10) fill in the rest. `#[non_exhaustive]` so they can be added additively.
    #[derive(Clone, Debug, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum SignalState {
        /// A fresh, valid reading.
        Ok,
        /// No reading has arrived yet.
        NoData,
        /// The device is in a low-power/sleep state.
        Asleep,
        /// The source doesn't expose this metric.
        Unsupported,
        /// The reading requires privileges we don't have.
        PermissionDenied,
        /// The last good reading is older than expected (the loop stalled or the source is failing).
        Stale {
            /// Time since the last good reading.
            age: Duration,
        },
        /// A remote host is unreachable (Phase 10).
        Offline {
            /// The unreachable host.
            host: String,
        },
    }

    /// An immutable, point-in-time view of every device — what a surface renders. Published through an
    /// `ArcSwap`; surfaces hold a cheap `Arc` clone and never touch a collector.
    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub struct Snapshot {
        /// The overall signal state for this sample (per-device states arrive in Phase 2).
        pub state: SignalState,
        /// One entry per device, in first-seen order.
        pub devices: Vec<DeviceSnapshot>,
    }

    /// One device's slice of a [`Snapshot`]: identity, the latest reading, and the bounded history the
    /// sparkline draws.
    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub struct DeviceSnapshot {
        /// Stable device index.
        pub index: u32,
        /// Human-readable name.
        pub name: String,
        /// The most recent reading, if any.
        pub latest: Option<Point>,
        /// Retained points, oldest-to-newest.
        pub history: Vec<Point>,
    }

    impl Snapshot {
        /// The pre-first-sample snapshot: no devices, `NoData`.
        fn empty() -> Self {
            Self {
                state: SignalState::NoData,
                devices: Vec::new(),
            }
        }

        /// Build an immutable snapshot from the live model, tagging it with `state`.
        fn from_model(model: &Model, state: SignalState) -> Self {
            let devices = model
                .devices()
                .iter()
                .map(|d| DeviceSnapshot {
                    index: d.index,
                    name: d.name.clone(),
                    latest: d.latest().copied(),
                    history: d.series().iter().copied().collect(),
                })
                .collect();
            Self { state, devices }
        }

        /// Build a one-shot snapshot from a single sample set — no loop, no `Model`. Lets the one-shot
        /// `ps` render from the same `Snapshot` type the GUI reads (the data-flow contract, §0.5).
        #[must_use]
        pub fn from_samples(samples: &[DeviceSample], state: SignalState) -> Self {
            let devices = samples
                .iter()
                .map(|s| {
                    let point = Point::new(s.ts, s.metrics);
                    DeviceSnapshot {
                        index: s.index,
                        name: s.name.clone(),
                        latest: Some(point),
                        history: vec![point],
                    }
                })
                .collect();
            Self { state, devices }
        }
    }

    /// Engine tunables. Introduced minimally here (§0.5's config grows per phase): the sample interval
    /// and how much history to retain.
    #[derive(Clone, Copy, Debug)]
    pub struct EngineConfig {
        /// How often the engine samples (the collector owns no clock).
        pub interval: Duration,
        /// How far back the ring buffers retain history.
        pub history: Duration,
    }

    impl Default for EngineConfig {
        fn default() -> Self {
            Self {
                interval: Duration::from_secs(1),
                history: Duration::from_secs(300),
            }
        }
    }

    impl EngineConfig {
        /// Ring-buffer capacity: `history / interval` (using the same floored interval the loop runs at),
        /// at least one slot.
        fn cap(self) -> usize {
            let interval = self.interval.max(MIN_INTERVAL).as_secs_f64();
            ((self.history.as_secs_f64() / interval).round() as usize).max(1)
        }
    }

    /// A running engine. Holds the published snapshot cell and the loop thread; dropping it stops the
    /// loop and joins cleanly.
    pub struct EngineHandle {
        snapshot: Arc<ArcSwap<Snapshot>>,
        shutdown: mpsc::Sender<()>,
        thread: Option<JoinHandle<()>>,
    }

    impl EngineHandle {
        /// The latest published snapshot — a cheap, lock-free `Arc` clone.
        #[must_use]
        pub fn latest(&self) -> Arc<Snapshot> {
            self.snapshot.load_full()
        }
    }

    impl Drop for EngineHandle {
        fn drop(&mut self) {
            // Wake the loop so it observes the shutdown and exits promptly, then join.
            let _ = self.shutdown.send(());
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    /// One sampling step — sample, stamp the engine clock, ingest, publish. Factored out of the loop so
    /// it's testable without a thread. `last_ok` tracks the last successful sample time, so a failure
    /// renders as `Stale{age}` (or `NoData` before any success) without dropping the retained history.
    fn tick(
        collector: &mut dyn Collector,
        model: &mut Model,
        cell: &ArcSwap<Snapshot>,
        last_ok: &mut Option<SystemTime>,
    ) {
        match collector.sample() {
            Ok(mut samples) => {
                // The engine owns the one clock (§0.5 R3): stamp every reading here, not in the source.
                let now = SystemTime::now();
                for s in &mut samples {
                    s.ts = now;
                }
                model.ingest(&samples);
                *last_ok = Some(now);
                cell.store(Arc::new(Snapshot::from_model(model, SignalState::Ok)));
            }
            Err(_e) => {
                let state = match *last_ok {
                    Some(t) => SignalState::Stale {
                        age: t.elapsed().unwrap_or_default(),
                    },
                    None => SignalState::NoData,
                };
                cell.store(Arc::new(Snapshot::from_model(model, state)));
            }
        }
    }

    /// Spawn the engine: one background thread that samples `collector` every `cfg.interval`, publishes
    /// an immutable [`Snapshot`], and calls `waker` after each publish so a reactive surface (the GUI)
    /// repaints on new data and idles otherwise. Returns once the thread is running.
    ///
    /// # Errors
    /// If the OS cannot spawn the engine thread.
    pub fn spawn(
        mut collector: Box<dyn Collector>,
        cfg: EngineConfig,
        waker: Box<dyn Fn() + Send + Sync>,
    ) -> std::io::Result<EngineHandle> {
        // Floor the interval so a misconfigured (e.g. zero) value can't turn `recv_timeout` into a
        // busy-spin — the monitor must never be a hog (keystone 6), whatever config later feeds in.
        let interval = cfg.interval.max(MIN_INTERVAL);
        let snapshot = Arc::new(ArcSwap::from_pointee(Snapshot::empty()));
        let (tx, rx) = mpsc::channel::<()>();
        let cap = cfg.cap();
        let cell = Arc::clone(&snapshot);

        let thread = std::thread::Builder::new()
            .name("agent-engine".to_owned())
            .spawn(move || {
                let mut model = Model::new(cap);
                let mut last_ok: Option<SystemTime> = None;
                loop {
                    tick(collector.as_mut(), &mut model, &cell, &mut last_ok);
                    waker();
                    match rx.recv_timeout(interval) {
                        Err(RecvTimeoutError::Timeout) => {}
                        // Shutdown signalled, or the handle (and its Sender) was dropped.
                        Err(RecvTimeoutError::Disconnected) | Ok(()) => break,
                    }
                }
            })?;

        Ok(EngineHandle {
            snapshot,
            shutdown: tx,
            thread: Some(thread),
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{Bytes, CollectError, DeviceSample, Metrics, Pct};

        /// A collector that yields one device with an increasing util and a sentinel `ts`, so we can
        /// prove the engine restamps.
        #[derive(Default)]
        struct StubCollector {
            n: u8,
        }
        impl Collector for StubCollector {
            fn name(&self) -> &str {
                "stub"
            }
            fn sample(&mut self) -> Result<Vec<DeviceSample>, CollectError> {
                self.n = self.n.wrapping_add(1);
                let metrics = Metrics::new(Pct::clamped(self.n), Bytes(1 << 30), Bytes(8 << 30));
                Ok(vec![DeviceSample::new(
                    0,
                    "Stub".to_owned(),
                    metrics,
                    SystemTime::UNIX_EPOCH, // the engine must overwrite this
                )])
            }
        }

        struct FailCollector;
        impl Collector for FailCollector {
            fn name(&self) -> &str {
                "fail"
            }
            fn sample(&mut self) -> Result<Vec<DeviceSample>, CollectError> {
                Err(CollectError::Source("boom".to_owned()))
            }
        }

        #[test]
        fn tick_advances_bounds_and_restamps() {
            let cell = ArcSwap::from_pointee(Snapshot::empty());
            let mut model = Model::new(3);
            let mut collector = StubCollector::default();
            let mut last_ok = None;
            for _ in 0..5 {
                tick(&mut collector, &mut model, &cell, &mut last_ok);
            }
            let snap = cell.load_full();
            assert!(matches!(snap.state, SignalState::Ok));
            let d = &snap.devices[0];
            assert_eq!(d.history.len(), 3); // bounded at cap, oldest dropped
            assert_eq!(d.latest.expect("a reading").metrics.util.get(), 5); // advanced
            assert_ne!(d.latest.expect("a reading").ts, SystemTime::UNIX_EPOCH); // engine restamped
            assert!(last_ok.is_some());
        }

        #[test]
        fn tick_failure_before_any_success_is_nodata() {
            let cell = ArcSwap::from_pointee(Snapshot::empty());
            let mut model = Model::new(3);
            let mut collector = FailCollector;
            let mut last_ok = None;
            tick(&mut collector, &mut model, &cell, &mut last_ok);
            assert!(matches!(cell.load_full().state, SignalState::NoData));
            assert!(last_ok.is_none());
        }

        #[test]
        fn tick_failure_after_success_is_stale_and_keeps_history() {
            let cell = ArcSwap::from_pointee(Snapshot::empty());
            let mut model = Model::new(3);
            let mut last_ok = None;

            tick(
                &mut StubCollector::default(),
                &mut model,
                &cell,
                &mut last_ok,
            ); // one good sample
            tick(&mut FailCollector, &mut model, &cell, &mut last_ok); // then a failure

            let snap = cell.load_full();
            assert!(matches!(snap.state, SignalState::Stale { .. }));
            // The retained history survives the failure — we degrade, we don't blank.
            assert_eq!(snap.devices[0].history.len(), 1);
        }

        #[test]
        fn spawn_publishes_then_joins_on_drop() {
            let cfg = EngineConfig {
                interval: Duration::from_millis(1),
                history: Duration::from_secs(1),
            };
            let handle = spawn(Box::new(StubCollector::default()), cfg, Box::new(|| {}))
                .expect("spawn the engine thread");

            // Bounded wait for the first published device — no fixed sleep, no flake.
            let mut published = false;
            for _ in 0..1000 {
                if !handle.latest().devices.is_empty() {
                    published = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(published, "engine should publish a device snapshot");
            drop(handle); // must join cleanly (no hang)
        }
    }
}

/// The machine-facing wire contract (§0.5 R6) — the stable shape of `ps --json`. Decoupled from the
/// internal [`Snapshot`](engine::Snapshot) so renderers evolve while the JSON stays additive-only:
/// renaming a field here is a SemVer-major break. Shared by the CLI now and the Phase 9 exporters later.
pub mod wire {
    use serde::Serialize;

    use super::engine::Snapshot;

    /// Bumped only on a breaking change to the JSON shape; new fields are additive within a version.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Top-level `ps --json` payload.
    #[derive(Debug, Serialize)]
    pub struct WireSnapshot {
        /// Contract version (see [`SCHEMA_VERSION`]).
        pub schema_version: u32,
        /// One entry per device that has a current reading.
        pub devices: Vec<WireDevice>,
    }

    /// One device's current values. Memory is bytes (lossless, source-native); the unit is in the field
    /// name so a consumer can't misread it.
    #[derive(Debug, Serialize)]
    pub struct WireDevice {
        /// Stable device index.
        pub index: u32,
        /// Human-readable name.
        pub name: String,
        /// GPU utilization, whole percent (`0..=100`).
        pub util_pct: u8,
        /// Memory in use, bytes.
        pub mem_used_bytes: u64,
        /// Total memory, bytes.
        pub mem_total_bytes: u64,
        // Optional metrics: `null` when the source/GPU doesn't expose them. The unit is in each field
        // name; values stay lossless integers. Added additively under schema_version 1.
        /// GPU die temperature, degrees Celsius.
        pub temperature_c: Option<u32>,
        /// Board power draw, milliwatts.
        pub power_mw: Option<u32>,
        /// SM (graphics) clock, megahertz.
        pub sm_clock_mhz: Option<u32>,
        /// Memory clock, megahertz.
        pub mem_clock_mhz: Option<u32>,
        /// SM occupancy, whole percent — `null` on base NVML (needs DCGM).
        pub sm_occupancy_pct: Option<u8>,
        /// PCIe transmit throughput, kilobytes per second.
        pub pcie_tx_kb_s: Option<u32>,
        /// PCIe receive throughput, kilobytes per second.
        pub pcie_rx_kb_s: Option<u32>,
        /// Fan speed, whole percent.
        pub fan_pct: Option<u8>,
    }

    impl WireSnapshot {
        /// Project a [`Snapshot`] onto the wire contract — current values only (no history), one entry
        /// per device that has a reading.
        #[must_use]
        pub fn from_snapshot(snapshot: &Snapshot) -> Self {
            let devices = snapshot
                .devices
                .iter()
                .filter_map(|d| {
                    d.latest.map(|p| {
                        let m = p.metrics;
                        WireDevice {
                            index: d.index,
                            name: d.name.clone(),
                            util_pct: m.util.get(),
                            mem_used_bytes: m.mem_used.get(),
                            mem_total_bytes: m.mem_total.get(),
                            temperature_c: m.temperature.map(|v| v.get()),
                            power_mw: m.power.map(|v| v.get()),
                            sm_clock_mhz: m.sm_clock.map(|v| v.get()),
                            mem_clock_mhz: m.mem_clock.map(|v| v.get()),
                            sm_occupancy_pct: m.sm_occupancy.map(|v| v.get()),
                            pcie_tx_kb_s: m.pcie_tx.map(|v| v.get()),
                            pcie_rx_kb_s: m.pcie_rx.map(|v| v.get()),
                            fan_pct: m.fan.map(|v| v.get()),
                        }
                    })
                })
                .collect();
            Self {
                schema_version: SCHEMA_VERSION,
                devices,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{Bytes, Celsius, DeviceSample, Metrics, Pct, SignalState};
        use std::time::SystemTime;

        #[test]
        fn schema_version_is_one() {
            assert_eq!(SCHEMA_VERSION, 1);
        }

        #[test]
        fn empty_snapshot_has_no_devices() {
            let snap = Snapshot::from_samples(&[], SignalState::Ok);
            assert!(WireSnapshot::from_snapshot(&snap).devices.is_empty());
        }

        #[test]
        fn from_snapshot_maps_device_fields() {
            // One present optional (temperature) and the rest absent, to prove both paths map.
            let metrics = Metrics::new(
                Pct::clamped(26),
                Bytes(5_300_000_000),
                Bytes(24_000_000_000),
            )
            .with_temperature(Some(Celsius(61)));
            let samples = vec![DeviceSample::new(
                0,
                "Mock GPU 0".to_owned(),
                metrics,
                SystemTime::UNIX_EPOCH,
            )];
            let wire =
                WireSnapshot::from_snapshot(&Snapshot::from_samples(&samples, SignalState::Ok));
            assert_eq!(wire.schema_version, 1);
            assert_eq!(wire.devices.len(), 1);
            let d = &wire.devices[0];
            assert_eq!(d.name, "Mock GPU 0");
            assert_eq!(
                (d.index, d.util_pct, d.mem_used_bytes, d.mem_total_bytes),
                (0, 26, 5_300_000_000, 24_000_000_000)
            );
            assert_eq!(d.temperature_c, Some(61)); // a present optional maps through
            assert_eq!(d.fan_pct, None); // an absent optional stays None (serialized as null)
        }
    }
}

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
        let metrics = Metrics::new(Pct::clamped(util), Bytes(1 << 30), Bytes(24 << 30));
        DeviceSample::new(index, format!("GPU {index}"), metrics, SystemTime::now())
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
        let utils: Vec<u8> = d.series().iter().map(|p| p.metrics.util.get()).collect();
        assert_eq!(utils, vec![2, 3, 4]); // ordered oldest→newest
        assert_eq!(d.latest().map(|p| p.metrics.util.get()), Some(4));
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
                .map(|p| p.metrics.util.get()),
            Some(21)
        );
        assert!(m.device(2).is_none());
    }

    #[test]
    fn error_display_is_stable() {
        // `cli` surfaces these via `to_string()` — guard the message against silent drift.
        assert_eq!(
            CollectError::Source("x".into()).to_string(),
            "collector error: x"
        );
        assert_eq!(SinkError::Publish("y".into()).to_string(), "sink error: y");
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

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Bounded: the series never holds more than `cap`, for any capacity (including 0) and any
        /// sequence of pushes.
        #[test]
        fn series_len_never_exceeds_cap(
            cap in 0usize..64,
            pushes in prop::collection::vec(any::<i32>(), 0..256),
        ) {
            let mut s = Series::new(cap);
            for v in &pushes {
                s.push(*v);
            }
            prop_assert!(s.len() <= cap);
        }

        /// Ordered + wraps: after any sequence of pushes, the series holds exactly the last `cap` values
        /// pushed, oldest-to-newest.
        #[test]
        fn series_keeps_last_cap_in_order(
            cap in 1usize..64,
            pushes in prop::collection::vec(any::<i32>(), 0..256),
        ) {
            let mut s = Series::new(cap);
            for v in &pushes {
                s.push(*v);
            }
            let got: Vec<i32> = s.iter().copied().collect();
            let want: Vec<i32> = pushes.iter().rev().take(cap).rev().copied().collect();
            prop_assert_eq!(got, want);
        }
    }
}
