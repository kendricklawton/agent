//! Integration: the real [`MockCollector`] drives the engine's headless sampling loop, and the model
//! advances — the Phase 1 "mock collector drives a headless loop" test, with no GPU.

use std::time::Duration;

use agent_collector::MockCollector;
use agent_core::engine::{self, EngineConfig};

/// Poll `handle.latest()` until `pred` holds or a bounded number of tries elapse (no fixed sleep, no
/// flake). Returns whether the predicate was observed.
fn wait_until(handle: &engine::EngineHandle, pred: impl Fn(&engine::Snapshot) -> bool) -> bool {
    for _ in 0..1000 {
        if pred(&handle.latest()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    false
}

#[test]
fn mock_collector_drives_the_engine_loop() {
    let handle = engine::spawn(
        Box::new(MockCollector::new(2)),
        EngineConfig {
            interval: Duration::from_millis(1),
            history: Duration::from_secs(1),
        },
        Box::new(|| {}),
    )
    .expect("spawn the engine thread");

    // The loop publishes the mock's two devices...
    assert!(
        wait_until(&handle, |s| s.devices.len() == 2),
        "engine should publish the mock's devices"
    );
    // ...and the model advances: each device's history grows past the first reading.
    assert!(
        wait_until(&handle, |s| s
            .devices
            .first()
            .is_some_and(|d| d.history.len() > 1)),
        "history should grow as the loop keeps sampling"
    );
    // Dropping the handle stops the loop and joins cleanly (no hang).
    drop(handle);
}
