use serde::{Deserialize, Serialize};

use argus_core_types::Side;
use argus_decimal::{LotSize, QtyLots};
use argus_schema::{CanonicalEvent, EventPayload};

/// Cumulative Volume Delta acumulador incremental.
///
/// Mantém soma de buy_qty - sell_qty em lots. Reset opcional por sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CvdAccumulator {
    /// CVD em lots (signed).
    pub cvd_lots: i128,
    /// Lot size para reconstrução em Decimal.
    pub lot_size: LotSize,
    /// Total buy lots.
    pub total_buy_lots: u128,
    /// Total sell lots.
    pub total_sell_lots: u128,
    /// Trades observados.
    pub trade_count: u64,
}

impl CvdAccumulator {
    /// Constrói com zero.
    pub fn new(lot_size: LotSize) -> Self {
        Self {
            cvd_lots: 0,
            lot_size,
            total_buy_lots: 0,
            total_sell_lots: 0,
            trade_count: 0,
        }
    }

    /// Aplica evento. Apenas trades modificam CVD; outros são ignorados
    /// silenciosamente.
    pub fn on_event(&mut self, event: &CanonicalEvent) {
        if let EventPayload::Trade(t) = &event.payload {
            let qty_lots = t.qty.lots;
            // qty.lots já é signed; pegamos magnitude e aplicamos signal de
            // side via signed().
            let magnitude = qty_lots.unsigned_abs();
            self.trade_count = self.trade_count.saturating_add(1);
            match t.aggressor {
                Side::Buy => {
                    self.cvd_lots = self.cvd_lots.saturating_add(qty_lots);
                    self.total_buy_lots = self.total_buy_lots.saturating_add(magnitude);
                }
                Side::Sell => {
                    self.cvd_lots = self.cvd_lots.saturating_sub(qty_lots);
                    self.total_sell_lots = self.total_sell_lots.saturating_add(magnitude);
                }
            }
        }
    }

    /// CVD atual como `QtyLots`.
    pub fn cvd(&self) -> QtyLots {
        QtyLots::from_lots(self.cvd_lots, self.lot_size)
    }

    /// Reset (ex: nova sessão).
    pub fn reset(&mut self) {
        self.cvd_lots = 0;
        self.total_buy_lots = 0;
        self.total_sell_lots = 0;
        self.trade_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::{EventId, InstrumentId, SequenceId, TradeId, Venue, VenueStatus};
    use argus_decimal::{PriceTicks, TickSize};
    use argus_schema::{DataQuality, TradePayload, SCHEMA_VERSION};
    use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};
    use rust_decimal_macros::dec;

    fn trade(side: Side, qty_lots: i128) -> CanonicalEvent {
        let ts = TickSize::new(dec!(0.01)).unwrap();
        let lot = LotSize::new(dec!(0.001)).unwrap();
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::from_raw(0),
            instrument: InstrumentId::from_u128(0),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(0),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(0)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(0)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(0)),
            venue_status: VenueStatus::Normal,
            quality: DataQuality::default(),
            payload: EventPayload::Trade(TradePayload {
                price: PriceTicks::from_ticks(5_000_000, ts),
                qty: QtyLots::from_lots(qty_lots, lot),
                aggressor: side,
                trade_id: TradeId::new("t"),
                aggressor_is_maker: false,
            }),
        }
    }

    #[test]
    fn cvd_accumulates_signed() {
        let lot = LotSize::new(dec!(0.001)).unwrap();
        let mut cvd = CvdAccumulator::new(lot);
        cvd.on_event(&trade(Side::Buy, 100));
        cvd.on_event(&trade(Side::Buy, 50));
        cvd.on_event(&trade(Side::Sell, 70));
        cvd.on_event(&trade(Side::Sell, 30));
        assert_eq!(cvd.cvd_lots, 50);
        assert_eq!(cvd.total_buy_lots, 150);
        assert_eq!(cvd.total_sell_lots, 100);
        assert_eq!(cvd.trade_count, 4);
    }

    #[test]
    fn reset_clears() {
        let lot = LotSize::new(dec!(0.001)).unwrap();
        let mut cvd = CvdAccumulator::new(lot);
        cvd.on_event(&trade(Side::Buy, 100));
        cvd.reset();
        assert_eq!(cvd.cvd_lots, 0);
        assert_eq!(cvd.trade_count, 0);
    }
}
