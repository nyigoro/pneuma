//! Week 14 runtime tests - async eval resolution via QuickJS job pump.
//!
//! These tests exercise the JS runtime in isolation: no Servo endpoints,
//! no broker service. The broker channel is created but never consumed.
//!
//! Run with:
//!   cargo test -p pneuma-core --test week14_async_eval_runtime

use anyhow::Result;
use pneuma_broker::handle::BrokerHandle;
use pneuma_js::Runtime;
use tokio::sync::mpsc;

/// Build a Runtime with a disconnected broker channel.
/// Sufficient for pure-JS promise tests that never call FFI navigate/evaluate.
fn make_runtime() -> Result<Runtime> {
    let (tx, _rx) = mpsc::unbounded_channel();
    Runtime::new(BrokerHandle::new(tx))
}

#[test]
fn eval_expression_resolves_async_promise() -> Result<()> {
    let rt = make_runtime()?;
    let result = rt.eval_expression("(async () => 1 + 2)()")?;
    assert_eq!(result, "3", "async IIFE should resolve to 3");
    Ok(())
}

#[test]
fn eval_expression_async_undefined_maps_to_null() -> Result<()> {
    let rt = make_runtime()?;
    let result = rt.eval_expression("(async () => undefined)()")?;
    assert_eq!(result, "null", "resolved undefined should serialize to null");
    Ok(())
}

#[test]
fn execute_script_pumps_microtasks() -> Result<()> {
    let rt = make_runtime()?;

    // Promise.resolve().then(...) schedules a microtask.
    // Without job pumping, __week14 stays 0 after execute_script returns.
    rt.execute_script(
        "globalThis.__week14 = 0; \
         Promise.resolve().then(function() { globalThis.__week14 = 42; });",
    )?;

    let result = rt.eval_expression("__week14")?;
    assert_eq!(
        result, "42",
        "microtask should have run before execute_script returned"
    );
    Ok(())
}

#[test]
fn eval_expression_infinite_microtask_hits_cap() {
    let rt = make_runtime().expect("runtime init failed");

    // A Promise that never resolves - chains itself indefinitely.
    // Should hit MAX_DRAIN_ITERATIONS and return an error, not hang.
    let result = rt.eval_expression(
        "(function() { \
            return new Promise(function(resolve) { \
                function loop() { Promise.resolve().then(loop); } \
                loop(); \
            }); \
         })()",
    );

    assert!(
        result.is_err(),
        "infinite microtask chain should produce an error"
    );
    let msg = result.expect_err("must fail").to_string();
    assert!(
        msg.contains("infinite") || msg.contains("cap") || msg.contains("did not resolve"),
        "error message should indicate cap/infinite-loop: got {msg}"
    );
}
