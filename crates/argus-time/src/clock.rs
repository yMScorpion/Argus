use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::stamps::{MonotonicNs, UnixNanos};

/// Trait abstraindo fontes de tempo.
///
/// Permite injeção de `TestClock` em testes sem polluting produção com
/// chamadas de mock.
pub trait Clock: Send + Sync {
    /// Wall clock (Unix epoch nanos).
    fn now_unix(&self) -> UnixNanos;

    /// Monotonic clock (nanos desde origem arbitrária).
    fn now_monotonic(&self) -> MonotonicNs;
}

/// Trait apenas para fontes monotônicas (subset).
pub trait MonotonicClock: Send + Sync {
    /// Monotonic clock.
    fn now_monotonic(&self) -> MonotonicNs;
}

impl<T: Clock> MonotonicClock for T {
    fn now_monotonic(&self) -> MonotonicNs {
        Clock::now_monotonic(self)
    }
}

/// Implementação real usando `SystemTime` e `Instant`.
#[derive(Debug)]
pub struct RealClock {
    /// Origem do monotonic clock em nanos desde Unix epoch (aproximação,
    /// estabelecida no boot). Permite que `MonotonicNs` seja u64 e ainda
    /// caber.
    monotonic_origin: std::time::Instant,
}

impl RealClock {
    /// Constrói com origem agora.
    pub fn new() -> Self {
        Self { monotonic_origin: std::time::Instant::now() }
    }
}

impl Default for RealClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for RealClock {
    fn now_unix(&self) -> UnixNanos {
        let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        // Trunca para u64. Saturate em 2554-07-21 quando u64 nanos overflows.
        let nanos = dur.as_nanos();
        UnixNanos::from_nanos(nanos.min(u64::MAX as u128) as u64)
    }

    fn now_monotonic(&self) -> MonotonicNs {
        let elapsed = self.monotonic_origin.elapsed();
        // Saturate em u64 nanos (~584 anos, irrelevante na prática).
        let nanos = elapsed.as_nanos();
        MonotonicNs::from_nanos(nanos.min(u64::MAX as u128) as u64)
    }
}

/// Clock controlável manualmente para testes.
///
/// Wall e monotonic são independentes — testes podem simular NTP jumps
/// avançando wall sem mexer em monotonic.
#[derive(Debug)]
pub struct TestClock {
    unix_ns: AtomicU64,
    monotonic_ns: AtomicU64,
}

impl TestClock {
    /// Constrói com tempo inicial.
    pub fn new(unix_ns: u64, monotonic_ns: u64) -> Self {
        Self {
            unix_ns: AtomicU64::new(unix_ns),
            monotonic_ns: AtomicU64::new(monotonic_ns),
        }
    }

    /// Avança wall clock.
    pub fn advance_unix(&self, ns: u64) {
        self.unix_ns.fetch_add(ns, Ordering::SeqCst);
    }

    /// Avança monotonic clock.
    pub fn advance_monotonic(&self, ns: u64) {
        self.monotonic_ns.fetch_add(ns, Ordering::SeqCst);
    }

    /// Avança ambos juntos.
    pub fn advance(&self, ns: u64) {
        self.advance_unix(ns);
        self.advance_monotonic(ns);
    }

    /// Define wall clock para timestamp absoluto (usado para simular NTP
    /// jump).
    pub fn set_unix(&self, unix_ns: u64) {
        self.unix_ns.store(unix_ns, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_unix(&self) -> UnixNanos {
        UnixNanos::from_nanos(self.unix_ns.load(Ordering::SeqCst))
    }

    fn now_monotonic(&self) -> MonotonicNs {
        MonotonicNs::from_nanos(self.monotonic_ns.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_clock_progresses() {
        let c = RealClock::new();
        let a = Clock::now_monotonic(&c);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = Clock::now_monotonic(&c);
        assert!(b.as_nanos() > a.as_nanos());
    }

    #[test]
    fn real_clock_unix_is_in_range() {
        let c = RealClock::new();
        let now = c.now_unix();
        // Sanidade: timestamp atual deve ser entre 2020 e 2100.
        let ts_2020: u64 = 1_577_836_800 * 1_000_000_000;
        let ts_2100: u64 = 4_102_444_800 * 1_000_000_000;
        assert!(now.as_nanos() > ts_2020);
        assert!(now.as_nanos() < ts_2100);
    }

    #[test]
    fn test_clock_advances_independently() {
        let c = TestClock::new(1000, 1000);
        c.advance_unix(500);
        assert_eq!(c.now_unix().as_nanos(), 1500);
        assert_eq!(Clock::now_monotonic(&c).as_nanos(), 1000);
        c.advance_monotonic(200);
        assert_eq!(Clock::now_monotonic(&c).as_nanos(), 1200);
    }

    #[test]
    fn test_clock_simulates_ntp_jump() {
        let c = TestClock::new(1000, 1000);
        c.set_unix(99_999_999);
        assert_eq!(c.now_unix().as_nanos(), 99_999_999);
        // Monotonic não foi afetado.
        assert_eq!(Clock::now_monotonic(&c).as_nanos(), 1000);
    }
}
