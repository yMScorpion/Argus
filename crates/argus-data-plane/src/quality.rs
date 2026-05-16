//! Data Quality tracker por canal.
//!
//! Conforme spec §6.8 ("Data Quality Engine"), cada canal exposto pelo Data
//! Plane mantém métricas rolling de source latency, sequence gaps, stale
//! events, e produz um score de confidence composto. Estas métricas vão
//! direto para a UI (data quality strip) e para auto-actions (pausar
//! strategy quando confidence < threshold).
//!
//! Esta versão é **agregada** — não substitui o [`argus_schema::DataQuality`]
//! por evento, mas roda em paralelo, mantendo o estado **do canal**.

use std::sync::atomic::{AtomicU64, Ordering};

use argus_capability::Confidence;
use argus_metrics::Histogram;
use argus_schema::CanonicalEvent;
use argus_time::UnixNanos;

/// Estatísticas rolling de um canal específico (venue + symbol + channel).
#[derive(Debug, Default)]
pub struct ChannelQuality {
    /// Total de eventos contabilizados.
    pub events_total: AtomicU64,
    /// Sequence gaps detectados (skip > 1).
    pub gaps_total: AtomicU64,
    /// Eventos out-of-order (exchange_ts retrógrado).
    pub out_of_order_total: AtomicU64,
    /// Reconnects desde boot.
    pub reconnects_total: AtomicU64,
    /// Eventos com flag stale.
    pub stale_total: AtomicU64,
    /// Latest exchange_ts visto (cumulative max).
    pub last_exchange_ts_ns: AtomicU64,
    /// Latest sequence_id visto.
    pub last_seq: AtomicU64,
    /// Histograma de source latency em microssegundos.
    pub source_latency_us: Histogram,
}

impl ChannelQuality {
    /// Constrói vazio.
    pub fn new() -> Self {
        Self {
            events_total: AtomicU64::new(0),
            gaps_total: AtomicU64::new(0),
            out_of_order_total: AtomicU64::new(0),
            reconnects_total: AtomicU64::new(0),
            stale_total: AtomicU64::new(0),
            last_exchange_ts_ns: AtomicU64::new(0),
            last_seq: AtomicU64::new(0),
            source_latency_us: Histogram::new(),
        }
    }

    /// Observa um novo evento canônico vindo do normalizer.
    pub fn observe(&self, event: &CanonicalEvent) {
        self.events_total.fetch_add(1, Ordering::Relaxed);

        // Source latency (recv - exchange).
        let exchange = event.exchange_ts.as_nanos();
        let recv = event.recv_ts.as_nanos();
        if recv >= exchange {
            let lat_us = (recv - exchange) / 1_000;
            self.source_latency_us.observe(lat_us);
        }

        // Sequence gap detection (skip > 1 vs último visto).
        let seq = event.sequence_id.raw();
        let prev = self.last_seq.load(Ordering::Relaxed);
        if prev != 0 && seq > prev.saturating_add(1) {
            self.gaps_total.fetch_add(1, Ordering::Relaxed);
        }
        // Out-of-order: seq retrógrado.
        if prev != 0 && seq < prev {
            self.out_of_order_total.fetch_add(1, Ordering::Relaxed);
        }
        // Atualiza last_seq se este é mais recente.
        if seq > prev {
            // CAS pode falhar sob concorrência; em SPSC sub-canal não importa.
            let _ = self.last_seq.compare_exchange(
                prev,
                seq,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }

        // Last exchange_ts.
        let prev_ts = self.last_exchange_ts_ns.load(Ordering::Relaxed);
        if exchange > prev_ts {
            let _ = self.last_exchange_ts_ns.compare_exchange(
                prev_ts,
                exchange,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }

        // Flag stale: spec exige checar quality flags.
        for f in &event.quality.flags {
            if matches!(f, argus_schema::QualityFlag::Stale) {
                self.stale_total.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    /// Marca um reconnect.
    pub fn note_reconnect(&self) {
        self.reconnects_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Computa confidence agregado deste canal.
    ///
    /// Heurística (calibrável):
    /// - Baseline 95 se sem incidentes.
    /// - -20 se houve qualquer gap nos últimos N events (proxied via
    ///   `gaps_total / events_total`).
    /// - -10 por reconnect recente.
    /// - -15 se latência P99 acima de threshold (10s aqui; spec calibra).
    /// - -30 se stale_total > 0.
    pub fn confidence(&self) -> Confidence {
        let mut score: i16 = 95;
        let events = self.events_total.load(Ordering::Relaxed);
        let gaps = self.gaps_total.load(Ordering::Relaxed);
        let reconnects = self.reconnects_total.load(Ordering::Relaxed);
        let stale = self.stale_total.load(Ordering::Relaxed);

        if events > 0 && gaps > 0 {
            let gap_ratio_pct = (gaps * 100) / events;
            // 1 gap em 1000 events = 0%; 1 gap em 100 = 1% → -2; 1 em 10 = 10% → -20.
            score -= (gap_ratio_pct as i16 * 2).min(40);
        }
        if reconnects > 0 {
            score -= 10;
        }
        if stale > 0 {
            score -= 30;
        }
        let snap = self.source_latency_us.snapshot();
        // P99 > 10s = severamente stale.
        if snap.count > 0 && snap.p99 > 10_000_000 {
            score -= 15;
        }
        Confidence::new(score.clamp(0, 100) as u8)
    }

    /// Idade do dado mais recente em ms vs `now`.
    pub fn age_ms(&self, now: UnixNanos) -> u32 {
        let last = self.last_exchange_ts_ns.load(Ordering::Relaxed);
        if last == 0 {
            return u32::MAX;
        }
        let now_ns = now.as_nanos();
        if now_ns <= last {
            return 0;
        }
        let diff_ns = now_ns - last;
        let diff_ms = diff_ns / 1_000_000;
        diff_ms.try_into().unwrap_or(u32::MAX)
    }

    /// Snapshot leve para exportação.
    pub fn snapshot(&self) -> ChannelQualitySnapshot {
        let lat = self.source_latency_us.snapshot();
        ChannelQualitySnapshot {
            events_total: self.events_total.load(Ordering::Relaxed),
            gaps_total: self.gaps_total.load(Ordering::Relaxed),
            out_of_order_total: self.out_of_order_total.load(Ordering::Relaxed),
            reconnects_total: self.reconnects_total.load(Ordering::Relaxed),
            stale_total: self.stale_total.load(Ordering::Relaxed),
            last_seq: self.last_seq.load(Ordering::Relaxed),
            last_exchange_ts_ns: self.last_exchange_ts_ns.load(Ordering::Relaxed),
            source_latency_us_p50: lat.p50,
            source_latency_us_p99: lat.p99,
            confidence: self.confidence(),
        }
    }
}

/// Snapshot imutável das métricas de um canal (para export Prometheus/UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelQualitySnapshot {
    /// Total events.
    pub events_total: u64,
    /// Gaps.
    pub gaps_total: u64,
    /// Out of order.
    pub out_of_order_total: u64,
    /// Reconnects.
    pub reconnects_total: u64,
    /// Stale events.
    pub stale_total: u64,
    /// Last seen sequence id.
    pub last_seq: u64,
    /// Last seen exchange ts (nanos).
    pub last_exchange_ts_ns: u64,
    /// Source latency P50 (us).
    pub source_latency_us_p50: u64,
    /// Source latency P99 (us).
    pub source_latency_us_p99: u64,
    /// Confidence agregado.
    pub confidence: Confidence,
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::{EventId, InstrumentId, SequenceId, Side, TradeId, Venue, VenueStatus};
    use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
    use argus_schema::{CanonicalEvent, DataQuality, EventPayload, QualityFlag, TradePayload, SCHEMA_VERSION};
    use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};
    use rust_decimal_macros::dec;

    fn event(seq: u64, exchange_ns: u64, recv_ns: u64, flags: &[QualityFlag]) -> CanonicalEvent {
        let mut q = DataQuality::default();
        for f in flags {
            q.mark(*f);
        }
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::from_raw(seq),
            instrument: InstrumentId::from_u128(1),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(seq),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(exchange_ns)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(recv_ns)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(recv_ns)),
            venue_status: VenueStatus::Normal,
            quality: q,
            payload: EventPayload::Trade(TradePayload {
                price: PriceTicks::from_ticks(100, TickSize::new(dec!(0.01)).unwrap()),
                qty: QtyLots::from_lots(1, LotSize::new(dec!(0.001)).unwrap()),
                aggressor: Side::Buy,
                trade_id: TradeId::new("t-1"),
                aggressor_is_maker: false,
            }),
        }
    }

    #[test]
    fn observe_increments_event_count() {
        let q = ChannelQuality::new();
        q.observe(&event(1, 1_000, 1_500, &[]));
        q.observe(&event(2, 2_000, 2_500, &[]));
        let snap = q.snapshot();
        assert_eq!(snap.events_total, 2);
        assert_eq!(snap.gaps_total, 0);
    }

    #[test]
    fn detects_sequence_gap() {
        let q = ChannelQuality::new();
        q.observe(&event(1, 1_000, 1_500, &[]));
        q.observe(&event(2, 2_000, 2_500, &[]));
        q.observe(&event(10, 3_000, 3_500, &[])); // gap
        let snap = q.snapshot();
        assert_eq!(snap.gaps_total, 1);
    }

    #[test]
    fn source_latency_observed() {
        let q = ChannelQuality::new();
        q.observe(&event(1, 1_000_000_000, 1_000_100_000, &[])); // 100 us
        let snap = q.snapshot();
        assert!(snap.source_latency_us_p99 >= 100);
    }

    #[test]
    fn confidence_drops_on_stale() {
        let q = ChannelQuality::new();
        let baseline = q.confidence().score;
        q.observe(&event(1, 1_000, 1_500, &[QualityFlag::Stale]));
        let with_stale = q.confidence().score;
        assert!(with_stale < baseline);
    }

    #[test]
    fn age_ms_zero_when_now_equals_last() {
        let q = ChannelQuality::new();
        q.observe(&event(1, 5_000_000_000, 5_000_000_000, &[]));
        let now = UnixNanos::from_nanos(5_000_000_000);
        assert_eq!(q.age_ms(now), 0);
    }

    #[test]
    fn age_ms_progresses() {
        let q = ChannelQuality::new();
        q.observe(&event(1, 5_000_000_000, 5_000_000_000, &[]));
        let now = UnixNanos::from_nanos(5_500_000_000); // +500ms
        assert_eq!(q.age_ms(now), 500);
    }
}
