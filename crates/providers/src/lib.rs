//! `agent-providers` — data-source adapters behind [`agent_core::DataProvider`].
//!
//! [`MockProvider`] returns a deterministic, known price series with no network — the keyless default and
//! the basis for the known-answer evals. Real adapters (Polygon, then Kalshi) map their raw HTTP responses
//! onto the canonical [`Bar`] schema here, which is what contains provider API drift.

#![forbid(unsafe_code)]

use std::time::{Duration, SystemTime};

use agent_core::{Bar, Capabilities, DataProvider, DataQuery, ProviderError};

/// Seconds in a day, for spacing the synthetic bars.
const DAY: u64 = 86_400;

/// A deterministic data source: for `last_days = n`, returns `n` daily bars whose close climbs
/// `100.0, 101.0, …` — so the average/latest/max/min are known exactly (the eval tests rely on this).
pub struct MockProvider;

impl DataProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::new(true)
    }

    fn bars(&mut self, query: &DataQuery) -> Result<Vec<Bar>, ProviderError> {
        let n = query.last_days.max(1);
        let bars = (0..n)
            .map(|i| {
                let close = 100.0 + f64::from(i);
                let ts = SystemTime::UNIX_EPOCH + Duration::from_secs(u64::from(i) * DAY);
                Bar::new(ts, close, close + 1.0, close - 1.0, close, 1_000)
            })
            .collect();
        Ok(bars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_requested_count_of_known_bars() {
        let mut p = MockProvider;
        let bars = p.bars(&DataQuery::new("FOO", 3)).expect("bars");
        assert_eq!(bars.len(), 3);
        assert!((bars[0].close - 100.0).abs() < 1e-9);
        assert!((bars[2].close - 102.0).abs() < 1e-9);
        assert!(p.capabilities().bars);
    }
}
