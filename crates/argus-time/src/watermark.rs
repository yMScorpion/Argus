use crate::stamps::UnixNanos;

/// Watermark monotônico para janelas temporais event-time.
///
/// Garante que computações dependentes de janela (candles, footprint,
/// agregações) só emitam quando todos os eventos esperados foram observados,
/// dentro de uma tolerância configurável.
///
/// # Modelo
///
/// Watermark `W(t)` indica: "todos os eventos com `exchange_ts <= W(t)` foram
/// processados (ou serão tratados como late)". Janelas que terminam em
/// timestamp `<= W(t)` podem ser fechadas.
///
/// Late events (`exchange_ts < W(t)`) ficam disponíveis em sidecar stream
/// `LateEvent` para auditoria; nunca afetam janelas já fechadas.
#[derive(Debug, Clone)]
pub struct Watermark {
    /// Timestamp atual do watermark.
    current: UnixNanos,
    /// Tolerância máxima admitida (max delay esperado).
    max_out_of_order_ns: u64,
}

impl Watermark {
    /// Constrói com watermark inicial em zero e tolerância dada.
    pub fn new(max_out_of_order_ns: u64) -> Self {
        Self { current: UnixNanos::from_nanos(0), max_out_of_order_ns }
    }

    /// Atualiza o watermark com base em um novo `exchange_ts` observado.
    ///
    /// Retorna `Some(new_watermark)` se avançou; `None` se não avançou.
    pub fn observe(&mut self, ts: UnixNanos) -> Option<UnixNanos> {
        // Watermark proposto = ts - max_out_of_order_ns (saturated).
        let proposed = UnixNanos::from_nanos(ts.as_nanos().saturating_sub(self.max_out_of_order_ns));
        if proposed > self.current {
            self.current = proposed;
            Some(self.current)
        } else {
            None
        }
    }

    /// Indica se um timestamp `ts` é considerado *late* (chegou depois que o
    /// watermark já passou dele).
    pub fn is_late(&self, ts: UnixNanos) -> bool {
        ts < self.current
    }

    /// Watermark atual.
    pub fn current(&self) -> UnixNanos {
        self.current
    }

    /// Tolerância máxima.
    pub fn tolerance_ns(&self) -> u64 {
        self.max_out_of_order_ns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watermark_advances_with_new_events() {
        let mut w = Watermark::new(1000);
        let advance = w.observe(UnixNanos::from_nanos(5000));
        assert_eq!(advance, Some(UnixNanos::from_nanos(4000)));
        assert_eq!(w.current(), UnixNanos::from_nanos(4000));
    }

    #[test]
    fn watermark_does_not_advance_for_older_event() {
        let mut w = Watermark::new(1000);
        w.observe(UnixNanos::from_nanos(5000));
        let advance = w.observe(UnixNanos::from_nanos(3000));
        assert_eq!(advance, None);
        assert_eq!(w.current(), UnixNanos::from_nanos(4000));
    }

    #[test]
    fn late_event_detection() {
        let mut w = Watermark::new(100);
        w.observe(UnixNanos::from_nanos(1000));
        // current watermark = 900
        assert!(w.is_late(UnixNanos::from_nanos(800)));
        assert!(!w.is_late(UnixNanos::from_nanos(950)));
        assert!(!w.is_late(UnixNanos::from_nanos(1500)));
    }

    #[test]
    fn watermark_is_monotonic() {
        let mut w = Watermark::new(100);
        let mut last = UnixNanos::from_nanos(0);
        for ts in [1000u64, 500, 2000, 1500, 3000, 100] {
            w.observe(UnixNanos::from_nanos(ts));
            assert!(w.current() >= last, "watermark retrocedeu");
            last = w.current();
        }
    }
}
