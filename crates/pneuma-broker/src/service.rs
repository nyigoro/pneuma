use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::mpsc;

use crate::confidence::{ConfidenceScorer, ConfidenceSignals, EngineDecision};
use crate::engine_factory::{DefaultEscalationEngineFactory, EscalationEngineFactory};
use crate::handle::BrokerRequest;
use pneuma_engines::HeadlessEngine;

/// Maximum time allowed for the full escalation handoff sequence:
/// extract_state -> create secondary -> bootstrap navigate -> import_state -> final navigate.
const ESCALATION_TIMEOUT: Duration = Duration::from_secs(10);
const ACTIVE_FAILURE_BUDGET: u32 = 3;
const ESCALATION_BACKOFF_AFTER_ROLLBACK: Duration = Duration::from_secs(30);
const RECOVERY_CONFIDENCE_THRESHOLD: f32 = 0.72;
const RECOVERY_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineRole {
    Primary,
    SecondaryProxy,
}

impl std::fmt::Display for EngineRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineRole::Primary => write!(f, "primary"),
            EngineRole::SecondaryProxy => write!(f, "secondary_proxy"),
        }
    }
}

struct BrokerState {
    active_engine: Box<dyn HeadlessEngine>,
    active_role: EngineRole,
    standby_primary: Option<Box<dyn HeadlessEngine>>,
    consecutive_failures: u32,
    escalation_backoff_until: Option<Instant>,
}

impl BrokerState {
    fn new(engine: Box<dyn HeadlessEngine>) -> Self {
        Self {
            active_engine: engine,
            active_role: EngineRole::Primary,
            standby_primary: None,
            consecutive_failures: 0,
            escalation_backoff_until: None,
        }
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Returns true when budget exhausted.
    fn record_failure(&mut self) -> bool {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.consecutive_failures >= ACTIVE_FAILURE_BUDGET
    }

    /// None = eligible. Some(reason) = suppressed.
    fn escalation_skip_reason(&self) -> Option<&'static str> {
        if self.active_role == EngineRole::SecondaryProxy {
            return Some("already_on_secondary");
        }
        if self.standby_primary.is_some() {
            return Some("standby_primary_present");
        }
        if let Some(until) = self.escalation_backoff_until {
            if Instant::now() < until {
                return Some("in_backoff_window");
            }
        }
        None
    }

    fn apply_escalation(&mut self, secondary: Box<dyn HeadlessEngine>) {
        let former = std::mem::replace(&mut self.active_engine, secondary);
        self.standby_primary = Some(former);
        self.active_role = EngineRole::SecondaryProxy;
        self.consecutive_failures = 0;
    }

    /// Returns the failed secondary for best-effort close by caller.
    fn apply_rollback(&mut self) -> Option<Box<dyn HeadlessEngine>> {
        let primary = self.standby_primary.take()?;
        let failed = std::mem::replace(&mut self.active_engine, primary);
        self.active_role = EngineRole::Primary;
        self.consecutive_failures = 0;
        self.escalation_backoff_until = Some(Instant::now() + ESCALATION_BACKOFF_AFTER_ROLLBACK);
        Some(failed)
    }

    /// Promote standby primary back to active after score-based recovery.
    /// Returns the former secondary for best-effort close by caller.
    /// Returns `None` if no standby is present.
    fn apply_score_recovery(&mut self) -> Option<Box<dyn HeadlessEngine>> {
        let primary = self.standby_primary.take()?;
        let old_secondary = std::mem::replace(&mut self.active_engine, primary);
        self.active_role = EngineRole::Primary;
        self.consecutive_failures = 0;
        // No backoff: this is a healthy transition, not a failure rollback.
        Some(old_secondary)
    }
}

struct HandoffResult {
    secondary: Box<dyn HeadlessEngine>,
    result_json: String,
    performed_final_navigate: bool,
    imported_entry_count: usize,
}

async fn close_standby_primary(state: &mut BrokerState) {
    if let Some(standby) = state.standby_primary.take() {
        if let Err(error) = standby.close().await {
            tracing::warn!(
                target: "pneuma_broker",
                error = %error,
                "failed to close standby primary"
            );
        }
    }
}

async fn handle_operation_health<T>(
    state: &mut BrokerState,
    page_id: u32,
    operation: &'static str,
    result: &anyhow::Result<T>,
) {
    match result {
        Ok(_) => state.record_success(),
        Err(_) => {
            if state.record_failure() && state.active_role == EngineRole::SecondaryProxy {
                tracing::warn!(
                    target: "pneuma_broker",
                    page_id,
                    operation,
                    consecutive_failures = state.consecutive_failures,
                    rollback_triggered = true,
                    "failure budget exhausted; rolling back to standby primary"
                );
                if let Some(failed) = state.apply_rollback() {
                    if let Err(error) = failed.close().await {
                        tracing::warn!(
                            target: "pneuma_broker",
                            error = %error,
                            "failed to close degraded secondary after rollback"
                        );
                    }
                } else {
                    tracing::warn!(
                        target: "pneuma_broker",
                        "rollback requested but standby primary was not available"
                    );
                }
            }
        }
    }
}

/// Attempt score-based recovery from secondary back to primary.
///
/// Called after successful navigations while on `SecondaryProxy`.
/// Recovery affects subsequent operations only; caller sends its current
/// navigate result unchanged.
async fn attempt_score_recovery(
    state: &mut BrokerState,
    scorer: &ConfidenceScorer,
    page_id: u32,
    url: &str,
    opts_json: &str,
    active_overall: f32,
) {
    if state.active_role != EngineRole::SecondaryProxy {
        return;
    }
    if state.standby_primary.is_none() {
        tracing::debug!(
            target: "pneuma_broker",
            page_id,
            active_overall,
            reason = "no_standby",
            "recovery probe skipped"
        );
        return;
    }
    if active_overall < RECOVERY_CONFIDENCE_THRESHOLD {
        tracing::debug!(
            target: "pneuma_broker",
            page_id,
            active_overall,
            threshold = RECOVERY_CONFIDENCE_THRESHOLD,
            reason = "below_threshold",
            "recovery probe skipped"
        );
        return;
    }

    let Some(standby) = state.standby_primary.take() else {
        return;
    };
    let probe_start = Instant::now();
    let probe_outcome =
        tokio::time::timeout(RECOVERY_PROBE_TIMEOUT, standby.navigate(url, opts_json)).await;
    let elapsed_ms = probe_start.elapsed().as_millis() as u64;

    match probe_outcome {
        Err(_timeout) => {
            tracing::warn!(
                target: "pneuma_broker",
                page_id,
                duration_ms = elapsed_ms,
                reason = "timeout",
                "recovery probe failed; staying on secondary"
            );
            state.standby_primary = Some(standby);
        }
        Ok(Err(error)) => {
            tracing::warn!(
                target: "pneuma_broker",
                page_id,
                duration_ms = elapsed_ms,
                error = %error,
                reason = "navigate_error",
                "recovery probe failed; staying on secondary"
            );
            state.standby_primary = Some(standby);
        }
        Ok(Ok(probe_meta)) => {
            let probe_signals = signals_from_navigate_meta(&probe_meta, page_id);
            let probe_report = scorer.score(&probe_signals);
            if probe_report.overall < RECOVERY_CONFIDENCE_THRESHOLD {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    probe_overall = probe_report.overall,
                    threshold = RECOVERY_CONFIDENCE_THRESHOLD,
                    duration_ms = elapsed_ms,
                    "recovery probe scored below threshold; staying on secondary"
                );
                state.standby_primary = Some(standby);
                return;
            }

            state.standby_primary = Some(standby);
            if let Some(old_secondary) = state.apply_score_recovery() {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    active_overall,
                    probe_overall = probe_report.overall,
                    duration_ms = elapsed_ms,
                    "score-based recovery succeeded; switched back to primary"
                );
                if let Err(error) = old_secondary.close().await {
                    tracing::warn!(
                        target: "pneuma_broker",
                        error = %error,
                        "failed to close secondary after score-based recovery"
                    );
                }
            }
        }
    }
}

/// Entry point used by `main.rs`. Wraps `run_with_factory` with the default factory.
pub async fn run(rx: mpsc::UnboundedReceiver<BrokerRequest>, engine: Box<dyn HeadlessEngine>) {
    run_with_factory(rx, engine, DefaultEscalationEngineFactory).await
}

/// Testable entry point that accepts an injected factory.
pub async fn run_with_factory<F>(
    mut rx: mpsc::UnboundedReceiver<BrokerRequest>,
    engine: Box<dyn HeadlessEngine>,
    factory: F,
) where
    F: EscalationEngineFactory + 'static,
{
    tracing::info!(target: "pneuma_broker", "service loop started");
    let scorer = ConfidenceScorer::new();
    let mut next_page_id: u32 = 1;
    let mut engine_closed = false;
    let mut state = BrokerState::new(engine);

    while let Some(req) = rx.recv().await {
        match req {
            BrokerRequest::CreatePage { reply } => {
                let page_id = next_page_id;
                next_page_id = next_page_id.saturating_add(1);
                tracing::info!(target: "pneuma_broker", page_id, "CreatePage");
                let _ = reply.send(Ok(page_id));
            }

            BrokerRequest::Navigate {
                page_id,
                url,
                opts_json,
                reply,
            } => {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    url = %url,
                    opts_len = opts_json.len(),
                    "Navigate"
                );

                let result = state.active_engine.navigate(&url, &opts_json).await;
                handle_operation_health(&mut state, page_id, "navigate", &result).await;

                // Stamp secondary-served responses before scoring or reply.
                let result = match result {
                    Ok(meta_json) if state.active_role == EngineRole::SecondaryProxy => {
                        Ok(stamp_migrated(&meta_json, true))
                    }
                    other => other,
                };

                let Ok(meta_json) = result.as_ref() else {
                    let _ = reply.send(result);
                    continue;
                };

                let signals = signals_from_navigate_meta(meta_json, page_id);
                let report = scorer.score(&signals);

                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    overall = report.overall,
                    paint = report.paint_score,
                    dom = report.dom_score,
                    js = report.js_score,
                    network = report.network_score,
                    decision = ?report.decision,
                    failure_reason = ?report.failure_reason,
                    "confidence report"
                );

                // While on secondary, probe standby primary for score-based recovery.
                // Current navigate result is returned unchanged either way.
                if state.active_role == EngineRole::SecondaryProxy {
                    if result.is_ok() {
                        attempt_score_recovery(
                            &mut state,
                            &scorer,
                            page_id,
                            &url,
                            &opts_json,
                            report.overall,
                        )
                        .await;
                    }
                    let _ = reply.send(result);
                    continue;
                }

                let escalation_decision = match &report.decision {
                    EngineDecision::EscalateToLadybird(reason) => Some(reason.clone()),
                    _ => None,
                };

                let Some(escalation_reason) = escalation_decision else {
                    // No escalation needed; reply with primary result immediately.
                    let _ = reply.send(result);
                    continue;
                };

                if let Some(skip_reason) = state.escalation_skip_reason() {
                    tracing::info!(
                        target: "pneuma_broker",
                        page_id,
                        escalation_skipped_reason = skip_reason,
                        active_role = %state.active_role,
                        standby_present = state.standby_primary.is_some(),
                        "escalation suppressed"
                    );
                    let _ = reply.send(result);
                    continue;
                }

                // Escalation path: one-shot, bounded, fallback on any failure.
                tracing::warn!(
                    target: "pneuma_broker",
                    page_id,
                    reason = ?escalation_reason,
                    "EscalateToLadybird decision; attempting handoff to secondary Servo proxy"
                );

                let handoff_start = Instant::now();

                let handoff_outcome = tokio::time::timeout(
                    ESCALATION_TIMEOUT,
                    perform_handoff(&*state.active_engine, &factory, &url, &opts_json),
                )
                .await;

                let elapsed_ms = handoff_start.elapsed().as_millis() as u64;

                match handoff_outcome {
                    Ok(Ok(handoff)) => {
                        // Log continuity signal: did the final page have a title?
                        let has_title = serde_json::from_str::<Value>(&handoff.result_json)
                            .ok()
                            .and_then(|v| {
                                v.get("title")
                                    .and_then(Value::as_str)
                                    .map(|t| !t.trim().is_empty())
                            })
                            .unwrap_or(false);

                        tracing::info!(
                            target: "pneuma_broker",
                            page_id,
                            reason = ?escalation_reason,
                            duration_ms = elapsed_ms,
                            secondary_engine = handoff.secondary.name(),
                            continuity_title_present = has_title,
                            performed_final_navigate = handoff.performed_final_navigate,
                            imported_entry_count = handoff.imported_entry_count,
                            "escalation handoff succeeded"
                        );

                        let final_result = stamp_migrated(&handoff.result_json, true);
                        state.apply_escalation(handoff.secondary);
                        let _ = reply.send(Ok(final_result));
                    }

                    Ok(Err(error)) => {
                        tracing::warn!(
                            target: "pneuma_broker",
                            page_id,
                            reason = ?escalation_reason,
                            duration_ms = elapsed_ms,
                            error = %error,
                            "escalation handoff failed; returning primary result"
                        );
                        let _ = reply.send(result);
                    }

                    Err(_timeout) => {
                        tracing::warn!(
                            target: "pneuma_broker",
                            page_id,
                            reason = ?escalation_reason,
                            duration_ms = elapsed_ms,
                            timeout_secs = ESCALATION_TIMEOUT.as_secs(),
                            "escalation handoff timed out; returning primary result"
                        );
                        let _ = reply.send(result);
                    }
                }
            }

            BrokerRequest::Evaluate {
                page_id,
                script,
                reply,
            } => {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    script_len = script.len(),
                    "Evaluate"
                );
                let result = state.active_engine.evaluate(&script).await;
                handle_operation_health(&mut state, page_id, "evaluate", &result).await;
                let _ = reply.send(result);
            }

            BrokerRequest::Screenshot { page_id, reply } => {
                tracing::info!(target: "pneuma_broker", page_id, "Screenshot");
                let result = state.active_engine.screenshot().await;
                handle_operation_health(&mut state, page_id, "screenshot", &result).await;
                let _ = reply.send(result);
            }

            BrokerRequest::SetViewport {
                page_id,
                width,
                height,
                reply,
            } => {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    width,
                    height,
                    active_role = %state.active_role,
                    "SetViewport"
                );
                let result = state.active_engine.set_viewport(width, height).await;
                handle_operation_health(&mut state, page_id, "set_viewport", &result).await;
                let _ = reply.send(result);
            }

            BrokerRequest::GetViewport { page_id, reply } => {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    active_role = %state.active_role,
                    "GetViewport"
                );
                let result = state.active_engine.get_viewport().await;
                handle_operation_health(&mut state, page_id, "get_viewport", &result).await;
                let _ = reply.send(result);
            }

            BrokerRequest::CloseBrowser { reply } => {
                tracing::info!(target: "pneuma_broker", "CloseBrowser");
                let result = state.active_engine.close().await;
                if result.is_ok() {
                    engine_closed = true;
                }
                close_standby_primary(&mut state).await;
                let _ = reply.send(result);
            }

            BrokerRequest::Shutdown { reply } => {
                tracing::info!(target: "pneuma_broker", "Shutdown - exiting service loop");
                let result = state.active_engine.close().await;
                if result.is_ok() {
                    engine_closed = true;
                }
                close_standby_primary(&mut state).await;
                let _ = reply.send(result);
                break;
            }
        }
    }

    if !engine_closed {
        if let Err(error) = state.active_engine.close().await {
            tracing::warn!(
                target: "pneuma_broker",
                error = %error,
                "engine close during service shutdown failed"
            );
        }
    }
    close_standby_primary(&mut state).await;

    tracing::info!(target: "pneuma_broker", "service loop exited");
}

/// Perform the full escalation handoff sequence:
///
/// 1. Extract state from the primary engine.
/// 2. Create a secondary engine via the factory.
/// 3. Bootstrap: navigate secondary to the target URL (establishes origin context).
/// 4. Import state into secondary.
/// 5. Final navigate to the target URL (now with restored state).
///
/// Returns `HandoffResult` on success.
/// Any failure propagates as `Err` and the caller falls back to primary.
async fn perform_handoff<F>(
    primary: &dyn HeadlessEngine,
    factory: &F,
    url: &str,
    opts_json: &str,
) -> anyhow::Result<HandoffResult>
where
    F: EscalationEngineFactory,
{
    // Step 1: capture state from primary.
    let state = primary
        .extract_state()
        .await
        .map_err(|e| anyhow::anyhow!("extract_state failed: {e}"))?;

    let cookie_count = state.cookies.len();
    let ls_count = state.local_storage.len();

    tracing::info!(
        target: "pneuma_broker",
        cookie_count,
        ls_entry_count = ls_count,
        current_url = ?state.current_url,
        "escalation: state captured from primary"
    );

    // Step 2: create secondary engine.
    let secondary = factory
        .create_for_escalation(pneuma_engines::EngineKind::Ladybird)
        .await
        .map_err(|e| anyhow::anyhow!("factory.create_for_escalation failed: {e}"))?;

    tracing::info!(
        target: "pneuma_broker",
        secondary_engine = secondary.name(),
        "escalation: secondary engine ready"
    );

    // Step 3: bootstrap navigate; establishes origin so cookie/LS context is valid.
    let bootstrap_result = secondary
        .navigate(url, opts_json)
        .await
        .map_err(|e| anyhow::anyhow!("secondary bootstrap navigate failed: {e}"))?;

    let entry_count = state.cookies.len() + state.local_storage.len();
    if state.cookies.is_empty() && state.local_storage.is_empty() {
        return Ok(HandoffResult {
            secondary,
            result_json: bootstrap_result,
            performed_final_navigate: false,
            imported_entry_count: 0,
        });
    }

    // Step 4: import state into secondary.
    secondary
        .import_state(state)
        .await
        .map_err(|e| anyhow::anyhow!("import_state failed: {e}"))?;

    tracing::info!(
        target: "pneuma_broker",
        cookie_count,
        ls_entry_count = ls_count,
        "escalation: state imported into secondary"
    );

    // Step 5: final navigate; now running with restored session state.
    let final_result = secondary
        .navigate(url, opts_json)
        .await
        .map_err(|e| anyhow::anyhow!("secondary final navigate failed: {e}"))?;

    Ok(HandoffResult {
        secondary,
        result_json: final_result,
        performed_final_navigate: true,
        imported_entry_count: entry_count,
    })
}

fn stamp_migrated(meta_json: &str, migrated: bool) -> String {
    let mut value: Value = match serde_json::from_str(meta_json) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => return meta_json.to_owned(),
    };
    value
        .as_object_mut()
        .unwrap()
        .insert("migrated".into(), Value::Bool(migrated));
    serde_json::to_string(&value).unwrap_or_else(|_| meta_json.to_owned())
}

fn signals_from_navigate_meta(meta_json: &str, page_id: u32) -> ConfidenceSignals {
    let sampled_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut signals = ConfidenceSignals {
        sampled_at_ms,
        ..Default::default()
    };

    let meta: Value = match serde_json::from_str(meta_json) {
        Ok(value) => value,
        Err(error) => {
            tracing::debug!(
                target: "pneuma_broker",
                page_id,
                error = %error,
                "failed to parse navigate metadata JSON"
            );
            return signals;
        }
    };

    let Some(object) = meta.as_object() else {
        tracing::debug!(
            target: "pneuma_broker",
            page_id,
            "navigate metadata was not a JSON object"
        );
        return signals;
    };

    let ok = object.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if ok {
        signals.first_paint_ms = Some(600);
    }

    let title = object
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if !title.is_empty() {
        signals.paint_element_count = 24;
        signals.dom_element_count = 32;
        signals.dom_depth_max = 6;
        signals.body_text_length = std::cmp::max(title.len() * 12, 64);
        signals.js_execution_time_ms = 250;
    }

    if let Some(value) = parse_u64(object, "first_paint_ms") {
        signals.first_paint_ms = Some(value);
    }
    if let Some(value) = parse_usize(object, "paint_element_count") {
        signals.paint_element_count = value;
    }
    if let Some(value) = parse_usize(object, "dom_element_count") {
        signals.dom_element_count = value;
    }
    if let Some(value) = parse_usize(object, "dom_depth_max") {
        signals.dom_depth_max = value;
    }
    if let Some(value) = parse_usize(object, "body_text_length") {
        signals.body_text_length = value;
    }

    if let Some(value) = parse_u32(object, "js_errors") {
        signals.js_errors = value;
    }
    if let Some(value) = parse_u32(object, "unhandled_promise_rejections") {
        signals.unhandled_promise_rejections = value;
    }
    if let Some(value) = parse_u32(object, "console_error_count") {
        signals.console_error_count = value;
    }
    if let Some(value) = parse_u64(object, "js_execution_time_ms") {
        signals.js_execution_time_ms = value;
    }
    if let Some(value) = parse_u32(object, "failed_resource_count") {
        signals.failed_resource_count = value;
    }
    if let Some(value) = parse_u32(object, "cors_violations") {
        signals.cors_violations = value;
    }
    if let Some(value) = parse_u32(object, "pending_requests_at_sample") {
        signals.pending_requests_at_sample = value;
    }
    if let Some(value) = parse_u32(object, "css_parse_failures") {
        signals.css_parse_failures = value;
    }

    signals
}

fn parse_u32(object: &serde_json::Map<String, Value>, key: &str) -> Option<u32> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value.min(u32::MAX as u64) as u32)
}

fn parse_u64(object: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn parse_usize(object: &serde_json::Map<String, Value>, key: &str) -> Option<usize> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value.min(usize::MAX as u64) as usize)
}

#[cfg(test)]
mod tests {
    use super::{
        attempt_score_recovery, signals_from_navigate_meta, stamp_migrated, BrokerState,
        EngineRole, ESCALATION_TIMEOUT, RECOVERY_CONFIDENCE_THRESHOLD,
    };
    use crate::confidence::ConfidenceScorer;
    use crate::engine_factory::EscalationEngineFactory;
    use anyhow::Result;
    use async_trait::async_trait;
    use pneuma_engines::{EngineKind, HeadlessEngine, MigrationEnvelope};
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc;

    #[test]
    fn valid_metadata_with_title_infers_signal_baseline() {
        let signals = signals_from_navigate_meta(r#"{"ok":true,"title":"Example Domain"}"#, 7);
        assert_eq!(signals.first_paint_ms, Some(600));
        assert!(signals.paint_element_count > 0);
        assert!(signals.dom_element_count > 0);
        assert!(signals.body_text_length >= 64);
    }

    #[test]
    fn invalid_metadata_returns_safe_defaults() {
        let signals = signals_from_navigate_meta("not-json", 11);
        assert_eq!(signals.first_paint_ms, None);
        assert_eq!(signals.paint_element_count, 0);
        assert_eq!(signals.dom_element_count, 0);
        assert_eq!(signals.js_errors, 0);
        assert_eq!(signals.failed_resource_count, 0);
    }

    #[test]
    fn optional_numeric_fields_are_ingested() {
        let signals = signals_from_navigate_meta(
            r#"{
                "ok": true,
                "title": "x",
                "js_errors": 4,
                "unhandled_promise_rejections": 3,
                "console_error_count": 2,
                "failed_resource_count": 8,
                "cors_violations": 5,
                "pending_requests_at_sample": 6,
                "css_parse_failures": 7,
                "js_execution_time_ms": 9001
            }"#,
            2,
        );
        assert_eq!(signals.js_errors, 4);
        assert_eq!(signals.unhandled_promise_rejections, 3);
        assert_eq!(signals.console_error_count, 2);
        assert_eq!(signals.failed_resource_count, 8);
        assert_eq!(signals.cors_violations, 5);
        assert_eq!(signals.pending_requests_at_sample, 6);
        assert_eq!(signals.css_parse_failures, 7);
        assert_eq!(signals.js_execution_time_ms, 9001);
    }

    #[test]
    fn probe_explicit_fields_override_inferred_baseline() {
        let signals = signals_from_navigate_meta(
            r#"{
                "ok": true,
                "title": "x",
                "first_paint_ms": 42,
                "paint_element_count": 7,
                "dom_element_count": 15,
                "dom_depth_max": 4,
                "body_text_length": 100,
                "js_execution_time_ms": 80
            }"#,
            5,
        );
        assert_eq!(signals.first_paint_ms, Some(42));
        assert_eq!(signals.paint_element_count, 7);
        assert_eq!(signals.dom_element_count, 15);
        assert_eq!(signals.dom_depth_max, 4);
        assert_eq!(signals.body_text_length, 100);
        assert_eq!(signals.js_execution_time_ms, 80);
    }

    #[test]
    fn stamp_migrated_inserts_field() {
        let input = r#"{"ok":true,"engine":"servo","migrated":false}"#;
        let output = stamp_migrated(input, true);
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["migrated"], serde_json::Value::Bool(true));
        assert_eq!(value["engine"], "servo");
    }

    #[test]
    fn stamp_migrated_invalid_input_unchanged() {
        let input = "not-json";
        assert_eq!(stamp_migrated(input, true), input);
    }

    #[test]
    fn backoff_active_suppresses_escalation() {
        let engine = Box::new(FakeEngine::happy("primary", "title"));
        let mut state = BrokerState::new(engine);
        state.escalation_backoff_until = Some(Instant::now() + Duration::from_secs(60));
        assert_eq!(state.escalation_skip_reason(), Some("in_backoff_window"));
    }

    #[test]
    fn backoff_expired_allows_escalation() {
        let engine = Box::new(FakeEngine::happy("primary", "title"));
        let mut state = BrokerState::new(engine);
        state.escalation_backoff_until = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(state.escalation_skip_reason(), None);
    }

    #[test]
    fn record_failure_reaches_budget() {
        let engine = Box::new(FakeEngine::happy("primary", "title"));
        let mut state = BrokerState::new(engine);
        state.active_role = EngineRole::SecondaryProxy;
        assert!(!state.record_failure());
        assert!(!state.record_failure());
        assert!(state.record_failure());
    }

    #[test]
    fn record_success_resets_counter() {
        let engine = Box::new(FakeEngine::happy("primary", "title"));
        let mut state = BrokerState::new(engine);
        state.record_failure();
        state.record_failure();
        state.record_success();
        assert!(!state.record_failure());
    }

    /// Engine with deterministic metadata output so scorer paths are controlled.
    struct MetaEngine {
        name: &'static str,
        meta: String,
        closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl MetaEngine {
        fn healthy(name: &'static str) -> Self {
            let meta = serde_json::json!({
                "ok": true,
                "engine": name,
                "title": "Healthy Page",
                "first_paint_ms": 300,
                "paint_element_count": 80,
                "dom_element_count": 120,
                "dom_depth_max": 8,
                "body_text_length": 2000,
                "js_execution_time_ms": 100,
                "js_errors": 0,
                "failed_resource_count": 0,
            })
            .to_string();
            Self {
                name,
                meta,
                closed: Default::default(),
            }
        }

        fn unhealthy(name: &'static str) -> Self {
            let meta = serde_json::json!({
                "ok": false,
                "engine": name,
                "title": "",
                "first_paint_ms": 9000,
                "paint_element_count": 0,
                "dom_element_count": 1,
                "body_text_length": 0,
                "js_errors": 0,
                "failed_resource_count": 0,
            })
            .to_string();
            Self {
                name,
                meta,
                closed: Default::default(),
            }
        }
    }

    #[async_trait]
    impl HeadlessEngine for MetaEngine {
        fn kind(&self) -> EngineKind {
            EngineKind::Servo
        }
        fn name(&self) -> &'static str {
            self.name
        }
        async fn navigate(&self, _url: &str, _opts: &str) -> Result<String> {
            Ok(self.meta.clone())
        }
        async fn evaluate(&self, _script: &str) -> Result<String> {
            Ok("null".into())
        }
        async fn screenshot(&self) -> Result<Vec<u8>> {
            Ok(vec![])
        }
        async fn set_viewport(&self, _w: u32, _h: u32) -> Result<()> {
            Ok(())
        }
        async fn get_viewport(&self) -> Result<(u32, u32)> {
            Ok((1280, 720))
        }
        async fn close(&self) -> Result<()> {
            self.closed.store(true, std::sync::atomic::Ordering::Release);
            Ok(())
        }
        async fn extract_state(&self) -> Result<MigrationEnvelope> {
            Ok(MigrationEnvelope {
                source_engine: EngineKind::Servo,
                captured_at_ms: 0,
                current_url: None,
                cookies: vec![],
                local_storage: vec![],
            })
        }
        async fn import_state(&self, _state: MigrationEnvelope) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn score_recovery_switches_to_primary_when_probe_is_healthy() {
        let scorer = ConfidenceScorer::new();
        let active = MetaEngine::healthy("secondary");
        let active_signals = signals_from_navigate_meta(&active.meta, 1);
        let report = scorer.score(&active_signals);
        assert!(
            report.overall >= RECOVERY_CONFIDENCE_THRESHOLD,
            "test setup must produce a healthy score"
        );

        let mut state = BrokerState::new(Box::new(active));
        state.active_role = EngineRole::SecondaryProxy;
        state.standby_primary = Some(Box::new(MetaEngine::healthy("standby_primary")));

        attempt_score_recovery(
            &mut state,
            &scorer,
            1,
            "https://example.com/",
            "{}",
            report.overall,
        )
        .await;

        assert_eq!(state.active_role, EngineRole::Primary);
        assert!(state.standby_primary.is_none());
    }

    #[tokio::test]
    async fn score_recovery_not_attempted_below_threshold() {
        let scorer = ConfidenceScorer::new();
        let mut state = BrokerState::new(Box::new(MetaEngine::healthy("secondary")));
        state.active_role = EngineRole::SecondaryProxy;
        state.standby_primary = Some(Box::new(MetaEngine::healthy("standby")));

        let below_threshold = RECOVERY_CONFIDENCE_THRESHOLD - 0.01;
        attempt_score_recovery(
            &mut state,
            &scorer,
            1,
            "https://example.com/",
            "{}",
            below_threshold,
        )
        .await;

        assert_eq!(state.active_role, EngineRole::SecondaryProxy);
        assert!(state.standby_primary.is_some());
    }

    #[tokio::test]
    async fn score_recovery_probe_failure_stays_on_secondary() {
        struct FailingStandby;
        #[async_trait]
        impl HeadlessEngine for FailingStandby {
            fn kind(&self) -> EngineKind {
                EngineKind::Servo
            }
            fn name(&self) -> &'static str {
                "failing_standby"
            }
            async fn navigate(&self, _url: &str, _opts: &str) -> Result<String> {
                Err(anyhow::anyhow!("standby probe failed"))
            }
            async fn evaluate(&self, _script: &str) -> Result<String> {
                Ok("null".into())
            }
            async fn screenshot(&self) -> Result<Vec<u8>> {
                Ok(vec![])
            }
            async fn set_viewport(&self, _w: u32, _h: u32) -> Result<()> {
                Ok(())
            }
            async fn get_viewport(&self) -> Result<(u32, u32)> {
                Ok((1280, 720))
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
            async fn extract_state(&self) -> Result<MigrationEnvelope> {
                Err(anyhow::anyhow!("not implemented"))
            }
            async fn import_state(&self, _state: MigrationEnvelope) -> Result<()> {
                Ok(())
            }
        }

        let scorer = ConfidenceScorer::new();
        let mut state = BrokerState::new(Box::new(MetaEngine::healthy("secondary")));
        state.active_role = EngineRole::SecondaryProxy;
        state.standby_primary = Some(Box::new(FailingStandby));

        let healthy = RECOVERY_CONFIDENCE_THRESHOLD + 0.10;
        attempt_score_recovery(
            &mut state,
            &scorer,
            1,
            "https://example.com/",
            "{}",
            healthy,
        )
        .await;

        assert_eq!(state.active_role, EngineRole::SecondaryProxy);
        assert!(state.standby_primary.is_some());
    }

    #[tokio::test]
    async fn score_recovery_probe_low_score_stays_on_secondary() {
        let scorer = ConfidenceScorer::new();
        let mut state = BrokerState::new(Box::new(MetaEngine::healthy("secondary")));
        state.active_role = EngineRole::SecondaryProxy;
        state.standby_primary = Some(Box::new(MetaEngine::unhealthy("weak_standby")));

        let healthy = RECOVERY_CONFIDENCE_THRESHOLD + 0.10;
        attempt_score_recovery(
            &mut state,
            &scorer,
            1,
            "https://example.com/",
            "{}",
            healthy,
        )
        .await;

        assert_eq!(state.active_role, EngineRole::SecondaryProxy);
        assert!(state.standby_primary.is_some());
    }

    struct FakeEngine {
        name: &'static str,
        navigate_result: Result<String>,
        extract_result: Result<MigrationEnvelope>,
        import_result: Result<()>,
        closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl FakeEngine {
        fn happy(name: &'static str, title: &str) -> Self {
            let meta = serde_json::json!({
                "ok": true,
                "engine": name,
                "title": title,
            })
            .to_string();
            let envelope = MigrationEnvelope {
                source_engine: EngineKind::Servo,
                captured_at_ms: 0,
                current_url: Some("https://example.com/".into()),
                cookies: vec![],
                local_storage: vec![],
            };
            FakeEngine {
                name,
                navigate_result: Ok(meta),
                extract_result: Ok(envelope),
                import_result: Ok(()),
                closed: Default::default(),
            }
        }

        fn failing_navigate(name: &'static str) -> Self {
            FakeEngine {
                name,
                navigate_result: Err(anyhow::anyhow!("navigate failed")),
                extract_result: Err(anyhow::anyhow!("extract failed")),
                import_result: Ok(()),
                closed: Default::default(),
            }
        }
    }

    #[async_trait]
    impl HeadlessEngine for FakeEngine {
        fn kind(&self) -> EngineKind {
            EngineKind::Servo
        }
        fn name(&self) -> &'static str {
            self.name
        }
        async fn navigate(&self, _url: &str, _opts: &str) -> Result<String> {
            match &self.navigate_result {
                Ok(s) => Ok(s.clone()),
                Err(e) => Err(anyhow::anyhow!("{e}")),
            }
        }
        async fn evaluate(&self, _script: &str) -> Result<String> {
            Ok("null".into())
        }
        async fn screenshot(&self) -> Result<Vec<u8>> {
            Ok(vec![])
        }
        async fn set_viewport(&self, _w: u32, _h: u32) -> Result<()> {
            Ok(())
        }
        async fn get_viewport(&self) -> Result<(u32, u32)> {
            Ok((1280, 720))
        }
        async fn close(&self) -> Result<()> {
            self.closed.store(true, std::sync::atomic::Ordering::Release);
            Ok(())
        }
        async fn extract_state(&self) -> Result<MigrationEnvelope> {
            match &self.extract_result {
                Ok(e) => Ok(e.clone()),
                Err(err) => Err(anyhow::anyhow!("{err}")),
            }
        }
        async fn import_state(&self, _state: MigrationEnvelope) -> Result<()> {
            match &self.import_result {
                Ok(()) => Ok(()),
                Err(e) => Err(anyhow::anyhow!("{e}")),
            }
        }
    }

    struct FakeFactory {
        engine: std::sync::Mutex<Option<Box<dyn HeadlessEngine>>>,
    }

    impl FakeFactory {
        fn with(engine: impl HeadlessEngine + 'static) -> Self {
            FakeFactory {
                engine: std::sync::Mutex::new(Some(Box::new(engine))),
            }
        }
    }

    #[async_trait]
    impl EscalationEngineFactory for FakeFactory {
        async fn create_for_escalation(&self, _target: EngineKind) -> Result<Box<dyn HeadlessEngine>> {
            let mut guard = self.engine.lock().map_err(|_| anyhow::anyhow!("factory lock poisoned"))?;
            guard.take().ok_or_else(|| anyhow::anyhow!("factory already consumed"))
        }
    }

    struct FailingFactory;

    #[async_trait]
    impl EscalationEngineFactory for FailingFactory {
        async fn create_for_escalation(&self, _target: EngineKind) -> Result<Box<dyn HeadlessEngine>> {
            Err(anyhow::anyhow!("factory failed"))
        }
    }

    #[tokio::test]
    async fn escalation_is_single_shot_per_navigate() {
        let primary = FakeEngine::happy("primary", "");
        let secondary = FakeEngine::happy("secondary", "Secondary Title");
        let factory = FakeFactory::with(secondary);

        let result = super::perform_handoff(
            &primary as &dyn HeadlessEngine,
            &factory,
            "https://example.com/",
            "{}",
        )
        .await;

        assert!(result.is_ok(), "handoff should succeed");
        let handoff = match result {
            Ok(values) => values,
            Err(error) => panic!("expected successful handoff, got error: {error}"),
        };
        assert_eq!(handoff.secondary.name(), "secondary");
        assert!(!handoff.performed_final_navigate);
        assert_eq!(handoff.imported_entry_count, 0);
        let v: serde_json::Value =
            serde_json::from_str(&handoff.result_json).expect("metadata should be JSON");
        let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("");
        assert_eq!(title, "Secondary Title");
    }

    #[tokio::test]
    async fn failed_factory_returns_error() {
        let primary = FakeEngine::happy("primary", "");
        let result = super::perform_handoff(
            &primary as &dyn HeadlessEngine,
            &FailingFactory,
            "https://example.com/",
            "{}",
        )
        .await;
        match result {
            Ok(_) => panic!("expected error"),
            Err(error) => assert!(error.to_string().contains("factory failed")),
        }
    }

    #[tokio::test]
    async fn failing_secondary_navigate_returns_error() {
        let primary = FakeEngine::happy("primary", "");
        let bad_secondary = FakeEngine::failing_navigate("bad_secondary");
        let factory = FakeFactory::with(bad_secondary);
        let result = super::perform_handoff(
            &primary as &dyn HeadlessEngine,
            &factory,
            "https://example.com/",
            "{}",
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn failing_extract_state_returns_error() {
        struct ExtractFailEngine;
        #[async_trait]
        impl HeadlessEngine for ExtractFailEngine {
            fn kind(&self) -> EngineKind {
                EngineKind::Servo
            }
            fn name(&self) -> &'static str {
                "extract_fail"
            }
            async fn navigate(&self, _: &str, _: &str) -> Result<String> {
                Ok(r#"{"ok":true,"title":"x"}"#.into())
            }
            async fn evaluate(&self, _: &str) -> Result<String> {
                Ok("null".into())
            }
            async fn screenshot(&self) -> Result<Vec<u8>> {
                Ok(vec![])
            }
            async fn set_viewport(&self, _w: u32, _h: u32) -> Result<()> {
                Ok(())
            }
            async fn get_viewport(&self) -> Result<(u32, u32)> {
                Ok((1280, 720))
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
            async fn extract_state(&self) -> Result<MigrationEnvelope> {
                Err(anyhow::anyhow!("extract deliberately failed"))
            }
            async fn import_state(&self, _: MigrationEnvelope) -> Result<()> {
                Ok(())
            }
        }

        let secondary = FakeEngine::happy("secondary", "Title");
        let factory = FakeFactory::with(secondary);
        let result = super::perform_handoff(
            &ExtractFailEngine as &dyn HeadlessEngine,
            &factory,
            "https://example.com/",
            "{}",
        )
        .await;
        match result {
            Ok(_) => panic!("expected extract_state failure"),
            Err(error) => assert!(error.to_string().contains("extract_state failed")),
        }
    }

    #[tokio::test]
    async fn timeout_falls_back_to_primary_result() {
        struct SlowEngine;
        #[async_trait]
        impl HeadlessEngine for SlowEngine {
            fn kind(&self) -> EngineKind {
                EngineKind::Servo
            }
            fn name(&self) -> &'static str {
                "slow"
            }
            async fn navigate(&self, _: &str, _: &str) -> Result<String> {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(r#"{"ok":true,"title":"slow"}"#.into())
            }
            async fn evaluate(&self, _: &str) -> Result<String> {
                Ok("null".into())
            }
            async fn screenshot(&self) -> Result<Vec<u8>> {
                Ok(vec![])
            }
            async fn set_viewport(&self, _w: u32, _h: u32) -> Result<()> {
                Ok(())
            }
            async fn get_viewport(&self) -> Result<(u32, u32)> {
                Ok((1280, 720))
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
            async fn extract_state(&self) -> Result<MigrationEnvelope> {
                tokio::time::sleep(ESCALATION_TIMEOUT + Duration::from_millis(20)).await;
                Ok(MigrationEnvelope {
                    source_engine: EngineKind::Servo,
                    captured_at_ms: 0,
                    current_url: None,
                    cookies: vec![],
                    local_storage: vec![],
                })
            }
            async fn import_state(&self, _: MigrationEnvelope) -> Result<()> {
                Ok(())
            }
        }

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(super::run_with_factory(rx, Box::new(SlowEngine), FailingFactory));
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let send_ok = tx.send(crate::handle::BrokerRequest::Navigate {
            page_id: 1,
            url: "https://example.com/".into(),
            opts_json: "{}".into(),
            reply: reply_tx,
        });
        assert!(send_ok.is_ok());

        let reply = reply_rx.await.expect("must receive navigate reply");
        assert!(reply.is_ok(), "expected fallback primary result on timeout/failure");
    }
}
