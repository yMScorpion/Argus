use serde::{Deserialize, Serialize};

use argus_core_types::{EventId, InstrumentId, SequenceId, Venue, VenueStatus};
use argus_time::{ExchangeTs, ProcessTs, RecvTs};

use crate::data_quality::DataQuality;
use crate::payload::EventPayload;

/// Evento canônico — unidade fundamental de market data dentro da suíte.
///
/// Todos os timestamps são separados: `exchange_ts` (venue diz que aconteceu),
/// `recv_ts` (chegou no nosso socket), `process_ts` (saiu do normalizer).
/// Sequencing é por (venue, instrument, payload kind).
///
/// Ver `docs/architecture/adr/ADR-0006-schema-format.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalEvent {
    /// Versão do schema deste evento (= [`crate::SCHEMA_VERSION`] na gravação).
    pub schema_version: u16,
    /// ID interno do evento (monotônico por canal).
    pub event_id: EventId,
    /// Instrumento ao qual o evento se refere.
    pub instrument: InstrumentId,
    /// Venue de origem.
    pub venue: Venue,
    /// Sequence id contínuo por canal.
    pub sequence_id: SequenceId,
    /// Timestamp informado pela venue.
    pub exchange_ts: ExchangeTs,
    /// Timestamp em que recebemos.
    pub recv_ts: RecvTs,
    /// Timestamp em que processamos (saiu do normalizer).
    pub process_ts: ProcessTs,
    /// Estado da venue no momento do evento.
    pub venue_status: VenueStatus,
    /// Qualidade do dado.
    pub quality: DataQuality,
    /// Payload concreto.
    pub payload: EventPayload,
}

impl CanonicalEvent {
    /// Latência da fonte em microssegundos.
    pub fn source_latency_us(&self) -> u64 {
        self.recv_ts
            .unix()
            .saturating_sub(self.exchange_ts.unix())
            / 1_000
    }

    /// Latência interna (normalize) em microssegundos.
    pub fn internal_latency_us(&self) -> u64 {
        self.process_ts
            .unix()
            .saturating_sub(self.recv_ts.unix())
            / 1_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::{EventPayload, TradePayload};
    use argus_core_types::{InstrumentId, Side, TradeId};
    use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
    use argus_time::UnixNanos;
    use rust_decimal_macros::dec;

    fn sample_event() -> CanonicalEvent {
        let ts = TickSize::new(dec!(0.01)).unwrap();
        let lot = LotSize::new(dec!(0.001)).unwrap();
        let trade = TradePayload {
            price: PriceTicks::from_ticks(5_000_000, ts),
            qty: QtyLots::from_lots(150, lot),
            aggressor: Side::Buy,
            trade_id: TradeId::new("t-1"),
            aggressor_is_maker: false,
        };
        CanonicalEvent {
            schema_version: crate::SCHEMA_VERSION,
            event_id: EventId::from_raw(1),
            instrument: InstrumentId::from_u128(0xDEAD_BEEF),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(42),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(1_000_000_000)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(1_000_050_000)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(1_000_100_000)),
            venue_status: VenueStatus::Normal,
            quality: DataQuality::default(),
            payload: EventPayload::Trade(trade),
        }
    }

    #[test]
    fn serde_roundtrip() {
        let e = sample_event();
        let j = serde_json::to_string(&e).unwrap();
        let back: CanonicalEvent = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn latency_decomposition() {
        let e = sample_event();
        // recv - exchange = 50_000 ns → 50 us → 50 / 1000 = 0 us em integer
        // truncation. Vamos verificar nanosecond version.
        assert_eq!(
            e.recv_ts.as_nanos() - e.exchange_ts.as_nanos(),
            50_000
        );
        assert_eq!(
            e.process_ts.as_nanos() - e.recv_ts.as_nanos(),
            50_000
        );
    }

    #[test]
    fn payload_classification() {
        let e = sample_event();
        assert!(e.payload.is_trade());
        assert!(!e.payload.touches_book());
    }
}
