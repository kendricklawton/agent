//! `gpumon-core` — the data model and the source of truth for the monitor.
//!
//! Device/process snapshots, bounded time series, and the [`Collector`] trait every data source
//! implements. **Every frontend (GUI/TUI/CLI) renders this** and nothing else — see ARCHITECTURE.md
//! (headless engine, pure-view frontends).

use std::collections::VecDeque;
use std::time::SystemTime;

/// A single point-in-time sample of one GPU device.
#[derive(Clone, Debug)]
pub struct DeviceSample {
    /// Stable device index (0-based).
    pub index: u32,
    /// Human-readable name (e.g. "NVIDIA H100").
    pub name: String,
    /// GPU utilization, 0–100%.
    pub util_pct: u8,
    /// Memory used, bytes.
    pub mem_used: u64,
    /// Total memory, bytes.
    pub mem_total: u64,
    /// When the sample was taken.
    pub ts: SystemTime,
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
/// Stubbed here as the export seam; implementations land in M8.
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
}
