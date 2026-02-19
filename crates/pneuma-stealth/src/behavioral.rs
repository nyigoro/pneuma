use rand::Rng;

pub fn jittered_delay_ms(base_ms: u64, variance_ms: u64) -> u64 {
    if variance_ms == 0 {
        return base_ms;
    }

    let mut rng = rand::thread_rng();
    let jitter = rng.gen_range(0..=variance_ms);
    base_ms.saturating_sub(variance_ms / 2).saturating_add(jitter)
}
