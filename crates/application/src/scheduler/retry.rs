use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base: Duration::from_secs(30),
            max_delay: Duration::from_secs(60 * 30),
        }
    }
}

impl RetryPolicy {
    /// Возвращает задержку перед `attempt`-й попыткой (0-индексирована).
    /// Формула: `min(base * 2^attempt + jitter, max_delay)`.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let exp = 2u64.saturating_pow(attempt.min(16));
        let raw = self.base.saturating_mul(exp.min(u32::MAX as u64) as u32);
        let jitter = Duration::from_millis(fastrand_jitter_ms(self.base.as_millis() as u64));
        raw.saturating_add(jitter).min(self.max_delay)
    }
}

fn fastrand_jitter_ms(base_ms: u64) -> u64 {
    // Простой LCG-подобный jitter без внешней зависимости.
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let s = STATE.fetch_add(0xA076_1D64_78BD_642F, Ordering::Relaxed);
    let mix = s ^ (s >> 33);
    mix.wrapping_mul(0xBF58_476D_1CE4_E5B9) % base_ms.max(1)
}
