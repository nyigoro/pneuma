use super::ConfidenceSignals;

#[derive(Debug, Default)]
pub struct ConfidenceScorer;

impl ConfidenceScorer {
    pub fn score(&self, signals: &ConfidenceSignals) -> f32 {
        let penalty = (signals.network_failures as f32 * 0.15) + (signals.js_errors as f32 * 0.1);
        (signals.base_score - penalty).clamp(0.0, 1.0)
    }
}
