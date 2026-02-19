use super::ConfidenceSignals;

#[derive(Debug, Clone, PartialEq)]
pub enum FailureReason {
    ZeroPaint,
    /// SPA pre-hydration stall — page shell loaded but JS hydration did not complete.
    /// Note: variant name spelling preserved for spec continuity; rename tracked separately.
    SpaPrehyrationStall,
    JsCrashLoop { error_count: u32 },
    NetworkStarvation { failed: u32 },
    CssLayoutCollapse,
    SlowExecution { ms: u64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineDecision {
    StayOnServo,
    EscalateToLadybird(FailureReason),
    RetryWithPatches(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct ConfidenceReport {
    pub paint_score: f32,
    pub dom_score: f32,
    pub js_score: f32,
    pub network_score: f32,
    pub overall: f32,
    pub failure_reason: Option<FailureReason>,
    pub decision: EngineDecision,
}

#[derive(Debug, Clone)]
pub struct ConfidenceScorer {
    pub escalation_threshold: f32,
}

impl Default for ConfidenceScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfidenceScorer {
    pub fn new() -> Self {
        Self {
            escalation_threshold: 0.60,
        }
    }

    pub fn with_threshold(threshold: f32) -> Self {
        Self {
            escalation_threshold: threshold,
        }
    }

    pub fn score(&self, signals: &ConfidenceSignals) -> ConfidenceReport {
        let paint = self.score_paint(signals);
        let dom = self.score_dom(signals);
        let js = self.score_js(signals);
        let network = self.score_network(signals);

        let overall = paint * 0.35 + dom * 0.30 + js * 0.25 + network * 0.10;

        let failure_reason = self.classify_failure(signals, paint, dom, js);
        let decision = self.decide(overall, &failure_reason, signals);

        ConfidenceReport {
            paint_score: paint,
            dom_score: dom,
            js_score: js,
            network_score: network,
            overall,
            failure_reason,
            decision,
        }
    }

    fn score_paint(&self, signals: &ConfidenceSignals) -> f32 {
        match (signals.first_paint_ms, signals.paint_element_count) {
            (None, _) => 0.0,
            (_, 0) => 0.1,
            (Some(ms), _) if ms > 8000 => 0.3,
            (Some(ms), _) if ms > 3000 => 0.6,
            (Some(_), count) => (count as f32 / 100.0).min(1.0) * 0.4 + 0.6,
        }
    }

    fn score_dom(&self, signals: &ConfidenceSignals) -> f32 {
        if signals.dom_element_count < 5 && signals.body_text_length < 50 {
            return 0.2;
        }
        if signals.dom_element_count < 20 {
            return 0.5;
        }
        (signals.dom_element_count as f32 / 200.0 + 0.5).min(1.0)
    }

    fn score_js(&self, signals: &ConfidenceSignals) -> f32 {
        let mut score = 1.0f32;
        score -= signals.unhandled_promise_rejections as f32 * 0.15;
        score -= signals.console_error_count as f32 * 0.05;
        score -= signals.js_errors as f32 * 0.10;
        score.max(0.0)
    }

    fn score_network(&self, signals: &ConfidenceSignals) -> f32 {
        let pending = (signals.pending_requests_at_sample as f32 * 0.05).min(0.3);
        let cors = (signals.cors_violations as f32 * 0.10).min(0.4);
        let failed = (signals.failed_resource_count as f32 * 0.03).min(0.2);
        (1.0 - pending - cors - failed).max(0.0)
    }

    fn classify_failure(
        &self,
        signals: &ConfidenceSignals,
        paint: f32,
        dom: f32,
        _js: f32,
    ) -> Option<FailureReason> {
        if paint == 0.0 {
            return Some(FailureReason::ZeroPaint);
        }
        if dom <= 0.2 {
            return Some(FailureReason::SpaPrehyrationStall);
        }
        if signals.js_errors > 3 || signals.unhandled_promise_rejections > 2 {
            return Some(FailureReason::JsCrashLoop {
                error_count: signals.js_errors,
            });
        }
        if signals.failed_resource_count > 5 || signals.cors_violations > 2 {
            return Some(FailureReason::NetworkStarvation {
                failed: signals.failed_resource_count,
            });
        }
        if signals.css_parse_failures > 3 {
            return Some(FailureReason::CssLayoutCollapse);
        }
        if signals.js_execution_time_ms > 5000 {
            return Some(FailureReason::SlowExecution {
                ms: signals.js_execution_time_ms,
            });
        }
        None
    }

    fn decide(
        &self,
        overall: f32,
        reason: &Option<FailureReason>,
        _signals: &ConfidenceSignals,
    ) -> EngineDecision {
        match reason {
            Some(FailureReason::SpaPrehyrationStall) => {
                return EngineDecision::EscalateToLadybird(FailureReason::SpaPrehyrationStall);
            }
            Some(reason) => return EngineDecision::EscalateToLadybird(reason.clone()),
            None => {}
        }

        if overall >= self.escalation_threshold {
            EngineDecision::StayOnServo
        } else {
            EngineDecision::EscalateToLadybird(FailureReason::ZeroPaint)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_signals() -> ConfidenceSignals {
        ConfidenceSignals {
            first_paint_ms: Some(450),
            paint_element_count: 80,
            dom_element_count: 40,
            body_text_length: 600,
            js_errors: 0,
            ..Default::default()
        }
    }

    #[test]
    fn healthy_page_stays_on_servo() {
        let scorer = ConfidenceScorer::new();
        let report = scorer.score(&healthy_signals());
        assert!(report.overall >= 0.60);
        assert_eq!(report.decision, EngineDecision::StayOnServo);
    }

    #[test]
    fn zero_paint_escalates() {
        let scorer = ConfidenceScorer::new();
        let signals = ConfidenceSignals {
            first_paint_ms: None,
            paint_element_count: 0,
            ..Default::default()
        };
        let report = scorer.score(&signals);
        assert_eq!(report.paint_score, 0.0);
        assert!(matches!(
            report.decision,
            EngineDecision::EscalateToLadybird(FailureReason::ZeroPaint)
        ));
    }

    #[test]
    fn spa_shell_escalates_immediately() {
        let scorer = ConfidenceScorer::new();
        let signals = ConfidenceSignals {
            first_paint_ms: Some(200),
            paint_element_count: 3,
            dom_element_count: 2,
            body_text_length: 10,
            ..Default::default()
        };
        let report = scorer.score(&signals);
        assert!(matches!(
            report.decision,
            EngineDecision::EscalateToLadybird(FailureReason::SpaPrehyrationStall)
        ));
    }

    #[test]
    fn js_crash_loop_escalates() {
        let scorer = ConfidenceScorer::new();
        let signals = ConfidenceSignals {
            first_paint_ms: Some(500),
            paint_element_count: 50,
            dom_element_count: 40,
            body_text_length: 500,
            js_errors: 5,
            ..Default::default()
        };
        let report = scorer.score(&signals);
        assert!(matches!(
            report.decision,
            EngineDecision::EscalateToLadybird(FailureReason::JsCrashLoop { .. })
        ));
    }

    #[test]
    fn custom_threshold_is_respected() {
        let scorer = ConfidenceScorer::with_threshold(0.95);
        let report = scorer.score(&healthy_signals());
        assert!(matches!(
            report.decision,
            EngineDecision::EscalateToLadybird(_)
        ));
    }
}
