//! Hot tier — eventos recentes em RAM.
//!
//! Ring buffer simples por símbolo. Substitui os "ring + arena" descritos na
//! spec por um `VecDeque` por enquanto — a interface é a mesma, e a versão
//! arena ficará para Fase 5.1 quando profilers indicarem que `VecDeque`
//! aloca demais.

use std::collections::VecDeque;

use argus_schema::CanonicalEvent;
use argus_time::UnixNanos;

/// Estatísticas leves do hot store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotStoreStats {
    /// Quantos eventos estão em RAM.
    pub events_in_ram: usize,
    /// Capacity máxima configurada.
    pub capacity: usize,
    /// Total de eventos descartados por overflow.
    pub overflowed: u64,
    /// Timestamp do evento mais antigo (None se vazio).
    pub oldest_exchange_ts_ns: Option<u64>,
    /// Timestamp do evento mais recente.
    pub newest_exchange_ts_ns: Option<u64>,
}

/// Store circular em RAM com idade máxima por símbolo.
///
/// Política: insere até `capacity`; quando cheio, descarta o mais antigo.
#[derive(Debug)]
pub struct HotStore {
    capacity: usize,
    events: VecDeque<CanonicalEvent>,
    overflowed: u64,
}

impl HotStore {
    /// Constrói com capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
            overflowed: 0,
        }
    }

    /// Insere um evento. Se cheio, evict o mais antigo.
    pub fn insert(&mut self, event: CanonicalEvent) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
            self.overflowed = self.overflowed.saturating_add(1);
        }
        self.events.push_back(event);
    }

    /// Retorna eventos dentro do intervalo `[from, to]` (inclusivo,
    /// exchange_ts). O((N)) — útil em janelas pequenas.
    pub fn range(&self, from: UnixNanos, to: UnixNanos) -> Vec<&CanonicalEvent> {
        self.events
            .iter()
            .filter(|e| {
                let t = e.exchange_ts.as_nanos();
                t >= from.as_nanos() && t <= to.as_nanos()
            })
            .collect()
    }

    /// Itera todos os eventos (oldest → newest).
    pub fn iter(&self) -> impl Iterator<Item = &CanonicalEvent> + '_ {
        self.events.iter()
    }

    /// Snapshot leve.
    pub fn stats(&self) -> HotStoreStats {
        HotStoreStats {
            events_in_ram: self.events.len(),
            capacity: self.capacity,
            overflowed: self.overflowed,
            oldest_exchange_ts_ns: self.events.front().map(|e| e.exchange_ts.as_nanos()),
            newest_exchange_ts_ns: self.events.back().map(|e| e.exchange_ts.as_nanos()),
        }
    }

    /// Quantos eventos.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Vazio?
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::{EventId, InstrumentId, SequenceId, Side, TradeId, Venue, VenueStatus};
    use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
    use argus_schema::{DataQuality, EventPayload, TradePayload, SCHEMA_VERSION};
    use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};
    use rust_decimal_macros::dec;

    fn event(seq: u64, ts_ns: u64) -> CanonicalEvent {
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::from_raw(seq),
            instrument: InstrumentId::from_u128(1),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(seq),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(ts_ns)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(ts_ns + 100)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(ts_ns + 200)),
            venue_status: VenueStatus::Normal,
            quality: DataQuality::default(),
            payload: EventPayload::Trade(TradePayload {
                price: PriceTicks::from_ticks(100, TickSize::new(dec!(0.01)).unwrap()),
                qty: QtyLots::from_lots(1, LotSize::new(dec!(0.001)).unwrap()),
                aggressor: Side::Buy,
                trade_id: TradeId::new(format!("t-{seq}")),
                aggressor_is_maker: false,
            }),
        }
    }

    #[test]
    fn insert_and_retrieve() {
        let mut s = HotStore::new(8);
        s.insert(event(1, 1_000));
        s.insert(event(2, 2_000));
        assert_eq!(s.len(), 2);
        let stats = s.stats();
        assert_eq!(stats.events_in_ram, 2);
        assert_eq!(stats.oldest_exchange_ts_ns, Some(1_000));
        assert_eq!(stats.newest_exchange_ts_ns, Some(2_000));
    }

    #[test]
    fn overflows_evict_oldest() {
        let mut s = HotStore::new(2);
        s.insert(event(1, 1_000));
        s.insert(event(2, 2_000));
        s.insert(event(3, 3_000));
        assert_eq!(s.len(), 2);
        assert_eq!(s.stats().oldest_exchange_ts_ns, Some(2_000));
        assert_eq!(s.stats().overflowed, 1);
    }

    #[test]
    fn range_filters_inclusive() {
        let mut s = HotStore::new(8);
        for i in 1..=5 {
            s.insert(event(i, i * 1000));
        }
        let r = s.range(UnixNanos::from_nanos(2000), UnixNanos::from_nanos(4000));
        assert_eq!(r.len(), 3);
    }
}
