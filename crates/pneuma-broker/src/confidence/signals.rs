use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceSignals {
    // Paint
    pub first_paint_ms: Option<u64>,
    pub paint_element_count: usize,

    // DOM
    pub dom_element_count: usize,
    pub dom_depth_max: usize,
    pub body_text_length: usize,

    // JS
    pub js_errors: u32,
    pub unhandled_promise_rejections: u32,
    pub console_error_count: u32,
    pub js_execution_time_ms: u64,

    // Network
    pub failed_resource_count: u32,
    pub cors_violations: u32,
    pub pending_requests_at_sample: u32,

    // CSS
    pub css_parse_failures: u32,

    // Timing
    pub sampled_at_ms: u64,
}

impl Default for ConfidenceSignals {
    fn default() -> Self {
        Self {
            first_paint_ms: None,
            paint_element_count: 0,
            dom_element_count: 0,
            dom_depth_max: 0,
            body_text_length: 0,
            js_errors: 0,
            unhandled_promise_rejections: 0,
            console_error_count: 0,
            js_execution_time_ms: 0,
            failed_resource_count: 0,
            cors_violations: 0,
            pending_requests_at_sample: 0,
            css_parse_failures: 0,
            sampled_at_ms: 0,
        }
    }
}
