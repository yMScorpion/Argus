use crossbeam_utils::CachePadded;
use std::sync::atomic::{AtomicU64, Ordering};

/// Contador monotônico thread-safe.
///
/// Padded para cache line, evitando false sharing entre updates concorrentes.
#[derive(Debug, Default)]
pub struct Counter(CachePadded<AtomicU64>);

impl Counter {
    /// Constrói com valor zero.
    pub const fn new() -> Self {
        Self(CachePadded::new(AtomicU64::new(0)))
    }

    /// Incrementa em `n`.
    pub fn inc(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    /// Incrementa em 1.
    pub fn tick(&self) {
        self.inc(1);
    }

    /// Lê valor atual.
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Gauge thread-safe (valor monotonicamente atualizável, não acumulativo).
#[derive(Debug, Default)]
pub struct Gauge(CachePadded<AtomicU64>);

impl Gauge {
    /// Constrói com valor zero.
    pub const fn new() -> Self {
        Self(CachePadded::new(AtomicU64::new(0)))
    }

    /// Define valor.
    pub fn set(&self, v: u64) {
        self.0.store(v, Ordering::Relaxed);
    }

    /// Lê valor.
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_increments() {
        let c = Counter::new();
        c.tick();
        c.inc(5);
        assert_eq!(c.get(), 6);
    }

    #[test]
    fn gauge_sets() {
        let g = Gauge::new();
        g.set(42);
        assert_eq!(g.get(), 42);
        g.set(7);
        assert_eq!(g.get(), 7);
    }

    #[test]
    fn counter_thread_safe() {
        use std::sync::Arc;
        let c = Arc::new(Counter::new());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let c = c.clone();
                std::thread::spawn(move || {
                    for _ in 0..10_000 {
                        c.tick();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(c.get(), 80_000);
    }
}
