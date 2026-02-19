use serde_json::Value;
use tokio::sync::mpsc;

use crate::confidence::{ConfidenceScorer, ConfidenceSignals, EngineDecision};
use crate::handle::BrokerRequest;
use pneuma_engines::HeadlessEngine;

pub async fn run(
    mut rx: mpsc::UnboundedReceiver<BrokerRequest>,
    engine: Box<dyn HeadlessEngine>,
) {
    tracing::info!(target: "pneuma_broker", "service loop started");
    let scorer = ConfidenceScorer::new();
    let mut next_page_id: u32 = 1;
    let mut engine_closed = false;

    while let Some(req) = rx.recv().await {
        match req {
            BrokerRequest::CreatePage { reply } => {
                let page_id = next_page_id;
                next_page_id = next_page_id.saturating_add(1);
                tracing::info!(target: "pneuma_broker", page_id, "CreatePage");
                let _ = reply.send(Ok(page_id));
            }
            // WEEK-7-ENTRY-POINT
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
                let result = engine.navigate(&url, &opts_json).await;

                if let Ok(ref meta_json) = result {
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

                    if let EngineDecision::EscalateToLadybird(reason) = &report.decision {
                        tracing::warn!(
                            target: "pneuma_broker",
                            page_id,
                            reason = ?reason,
                            "escalation indicated - Ladybird not yet available, staying on Servo"
                        );
                    }
                }

                let _ = reply.send(result);
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
                let result = engine.evaluate(&script).await;
                let _ = reply.send(result);
            }
            BrokerRequest::Screenshot { page_id, reply } => {
                tracing::info!(target: "pneuma_broker", page_id, "Screenshot");
                let result = engine.screenshot().await;
                let _ = reply.send(result);
            }
            BrokerRequest::CloseBrowser { reply } => {
                tracing::info!(target: "pneuma_broker", "CloseBrowser");
                let result = engine.close().await;
                if result.is_ok() {
                    engine_closed = true;
                }
                let _ = reply.send(result);
            }
            BrokerRequest::Shutdown { reply } => {
                tracing::info!(target: "pneuma_broker", "Shutdown - exiting service loop");
                let result = engine.close().await;
                if result.is_ok() {
                    engine_closed = true;
                }
                let _ = reply.send(result);
                break;
            }
        }
    }

    if !engine_closed {
        if let Err(error) = engine.close().await {
            tracing::warn!(
                target: "pneuma_broker",
                error = %error,
                "engine close during service shutdown failed"
            );
        }
    }

    tracing::info!(target: "pneuma_broker", "service loop exited");
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
    use super::signals_from_navigate_meta;

    #[test]
    fn valid_metadata_with_title_infers_signal_baseline() {
        let signals = signals_from_navigate_meta(
            r#"{"ok":true,"title":"Example Domain"}"#,
            7,
        );

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
}
