//! Publisher SHM para canonical events.
//!
//! Cada canal subscrito tem um `SpscRing<CanonicalEvent>` dedicado ligando o
//! Data Plane → Terminal (ou outros consumidores).
//!
//! Conforme spec §2.3, raw events e book deltas usam política `NEVER DROP`
//! (`WaitProducer`). Derivados (candles, footprint) podem usar `DropOldest`
//! com coalesce explícito, mas isso fica para a Fase 9.
//!
//! Esta abstração esconde o owner do `Arc<SpscRing>` — o Data Plane fica com
//! o Producer; o consumidor recebe o Consumer via `attach`.

use std::sync::Arc;

use argus_schema::CanonicalEvent;
use argus_shm::{Consumer, OverflowPolicy, Producer, ShmRingError, SpscRing};

/// Canal nomeado de canonical events publicado pelo Data Plane.
///
/// `attach` é chamado uma vez pelo consumidor (Terminal); para criar mais de
/// um consumidor seria preciso fan-out via SHM-MPMC, fora do escopo da Fase 3.
#[derive(Debug)]
pub struct PublishedChannel {
    name: String,
    producer: Producer<CanonicalEvent>,
    consumer: Option<Consumer<CanonicalEvent>>,
}

impl PublishedChannel {
    /// Constrói canal novo com capacity dada e política `NEVER DROP`.
    pub fn never_drop(name: impl Into<String>, capacity: usize) -> Result<Self, ShmRingError> {
        Self::with_policy(name, capacity, OverflowPolicy::WaitProducer)
    }

    /// Constrói canal novo com capacity e política customizada.
    pub fn with_policy(
        name: impl Into<String>,
        capacity: usize,
        policy: OverflowPolicy,
    ) -> Result<Self, ShmRingError> {
        let ring = SpscRing::<CanonicalEvent>::new(capacity, policy)?;
        let arc_clone: Arc<SpscRing<CanonicalEvent>> = ring;
        let (p, c) = arc_clone.split();
        Ok(Self {
            name: name.into(),
            producer: p,
            consumer: Some(c),
        })
    }

    /// Nome do canal (para logs/métricas).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Cede o consumidor ao caller; só pode ser chamado uma vez.
    pub fn attach_consumer(&mut self) -> Option<Consumer<CanonicalEvent>> {
        self.consumer.take()
    }

    /// Capacity do ring.
    pub fn capacity(&self) -> usize {
        self.producer.capacity()
    }

    /// Total de eventos descartados (apenas relevante em políticas drop).
    pub fn dropped(&self) -> u64 {
        self.producer.dropped()
    }

    /// Publica um evento. Em política `WaitProducer` pode bloquear (busy
    /// spin) se consumer estiver lento — chamada do Data Plane deve garantir
    /// que o consumidor está rodando.
    pub fn publish(&self, event: CanonicalEvent) -> Result<bool, ShmRingError> {
        self.producer.push(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::{EventId, InstrumentId, SequenceId, Side, TradeId, Venue, VenueStatus};
    use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
    use argus_schema::{
        DataQuality, EventPayload, TradePayload, SCHEMA_VERSION,
    };
    use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};
    use rust_decimal_macros::dec;

    fn sample_event(seq: u64) -> CanonicalEvent {
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::from_raw(seq),
            instrument: InstrumentId::from_u128(1),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(seq),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(seq * 1000)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(seq * 1000 + 100)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(seq * 1000 + 200)),
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
    fn publish_drain_roundtrip() {
        let mut ch = PublishedChannel::never_drop("test", 16).unwrap();
        let consumer = ch.attach_consumer().expect("first call gives consumer");
        ch.publish(sample_event(1)).unwrap();
        ch.publish(sample_event(2)).unwrap();
        let e1 = consumer.pop().expect("event 1");
        let e2 = consumer.pop().expect("event 2");
        assert_eq!(e1.sequence_id.raw(), 1);
        assert_eq!(e2.sequence_id.raw(), 2);
    }

    #[test]
    fn consumer_only_attached_once() {
        let mut ch = PublishedChannel::never_drop("test", 8).unwrap();
        assert!(ch.attach_consumer().is_some());
        assert!(ch.attach_consumer().is_none());
    }
}
