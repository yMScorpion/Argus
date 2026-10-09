use serde::{Deserialize, Serialize};

use argus_decimal::{PriceTicks, QtyLots, TickSize, LotSize};
use argus_schema::{CanonicalEvent, EventPayload};
use argus_time::UnixNanos;

/// Timeframe canônico em segundos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Timeframe {
    /// Segundos por candle.
    pub seconds: u32,
}

impl Timeframe {
    /// 1s.
    pub const ONE_SECOND: Self = Self { seconds: 1 };
    /// 5s.
    pub const FIVE_SECONDS: Self = Self { seconds: 5 };
    /// 1m.
    pub const ONE_MINUTE: Self = Self { seconds: 60 };
    /// 5m.
    pub const FIVE_MINUTES: Self = Self { seconds: 300 };
    /// 15m.
    pub const FIFTEEN_MINUTES: Self = Self { seconds: 900 };
    /// 1h.
    pub const ONE_HOUR: Self = Self { seconds: 3600 };

    /// Computa o início do candle ao qual `ts` pertence (alinhado).
    pub fn align(self, ts: UnixNanos) -> UnixNanos {
        let bucket_ns = self.seconds as u64 * 1_000_000_000;
        let aligned = (ts.as_nanos() / bucket_ns) * bucket_ns;
        UnixNanos::from_nanos(aligned)
    }
}

/// Candle OHLCV agregado.
///
/// Quantidades preservadas em `QtyLots` (i128) para evitar drift por
/// acumulação em float.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candle {
    /// Início do candle.
    pub open_ts: UnixNanos,
    /// Fim do candle (exclusivo).
    pub close_ts: UnixNanos,
    /// Open price.
    pub open: PriceTicks,
    /// High price.
    pub high: PriceTicks,
    /// Low price.
    pub low: PriceTicks,
    /// Close price.
    pub close: PriceTicks,
    /// Volume base asset.
    pub volume: QtyLots,
    /// Volume buy aggressor.
    pub volume_buy: QtyLots,
    /// Volume sell aggressor.
    pub volume_sell: QtyLots,
    /// Número de trades.
    pub trade_count: u32,
}

/// Builder incremental que produz candles a partir de eventos.
#[derive(Debug)]
pub struct CandleBuilder {
    timeframe: Timeframe,
    tick_size: TickSize,
    lot_size: LotSize,
    current: Option<CandleInProgress>,
}

#[derive(Debug)]
struct CandleInProgress {
    open_ts: UnixNanos,
    open: PriceTicks,
    high: PriceTicks,
    low: PriceTicks,
    close: PriceTicks,
    volume: QtyLots,
    volume_buy: QtyLots,
    volume_sell: QtyLots,
    trade_count: u32,
}

impl CandleBuilder {
    /// Constrói.
    pub fn new(timeframe: Timeframe, tick_size: TickSize, lot_size: LotSize) -> Self {
        Self { timeframe, tick_size, lot_size, current: None }
    }

    /// Aplica um `CanonicalEvent`. Retorna candle fechado se o evento
    /// extrapolou janela atual.
    pub fn on_event(&mut self, event: &CanonicalEvent) -> Option<Candle> {
        let trade = match &event.payload {
            EventPayload::Trade(t) => t,
            _ => return None,
        };
        let event_ts = event.exchange_ts.unix();
        let bucket_start = self.timeframe.align(event_ts);
        let bucket_end_ns = bucket_start.as_nanos() + self.timeframe.seconds as u64 * 1_000_000_000;

        // Se há candle atual e este evento pertence à mesma janela, agrega.
        if let Some(cur) = &mut self.current {
            if cur.open_ts == bucket_start {
                // Mesma janela; agrega.
                if trade.price > cur.high {
                    cur.high = trade.price;
                }
                if trade.price < cur.low {
                    cur.low = trade.price;
                }
                cur.close = trade.price;
                cur.volume = cur.volume.checked_add(trade.qty).unwrap_or(cur.volume);
                match trade.aggressor {
                    argus_core_types::Side::Buy => {
                        cur.volume_buy = cur.volume_buy.checked_add(trade.qty).unwrap_or(cur.volume_buy);
                    }
                    argus_core_types::Side::Sell => {
                        cur.volume_sell = cur.volume_sell.checked_add(trade.qty).unwrap_or(cur.volume_sell);
                    }
                }
                cur.trade_count = cur.trade_count.saturating_add(1);
                return None;
            }
        }

        // Janela mudou. Fecha anterior (se existe) e cria novo.
        let closed = self.current.take().map(|cur| Candle {
            open_ts: cur.open_ts,
            close_ts: UnixNanos::from_nanos(cur.open_ts.as_nanos() + self.timeframe.seconds as u64 * 1_000_000_000),
            open: cur.open,
            high: cur.high,
            low: cur.low,
            close: cur.close,
            volume: cur.volume,
            volume_buy: cur.volume_buy,
            volume_sell: cur.volume_sell,
            trade_count: cur.trade_count,
        });

        let (buy_initial, sell_initial) = match trade.aggressor {
            argus_core_types::Side::Buy => (trade.qty, QtyLots::from_lots(0, self.lot_size)),
            argus_core_types::Side::Sell => (QtyLots::from_lots(0, self.lot_size), trade.qty),
        };

        self.current = Some(CandleInProgress {
            open_ts: bucket_start,
            open: trade.price,
            high: trade.price,
            low: trade.price,
            close: trade.price,
            volume: trade.qty,
            volume_buy: buy_initial,
            volume_sell: sell_initial,
            trade_count: 1,
        });

        let _ = (self.tick_size, bucket_end_ns); // mantém fields usados
        closed
    }

    /// Força fechamento do candle atual.
    pub fn flush(&mut self) -> Option<Candle> {
        self.current.take().map(|cur| Candle {
            open_ts: cur.open_ts,
            close_ts: UnixNanos::from_nanos(
                cur.open_ts.as_nanos() + self.timeframe.seconds as u64 * 1_000_000_000,
            ),
            open: cur.open,
            high: cur.high,
            low: cur.low,
            close: cur.close,
            volume: cur.volume,
            volume_buy: cur.volume_buy,
            volume_sell: cur.volume_sell,
            trade_count: cur.trade_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core_types::{EventId, InstrumentId, SequenceId, Side, TradeId, Venue, VenueStatus};
    use argus_schema::{DataQuality, TradePayload, SCHEMA_VERSION};
    use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};
    use rust_decimal_macros::dec;

    fn ts() -> TickSize {
        TickSize::new(dec!(0.01)).unwrap()
    }

    fn lot() -> LotSize {
        LotSize::new(dec!(0.001)).unwrap()
    }

    fn trade_at(t_ns: u64, price_ticks: i128, qty_lots: i128, side: Side) -> CanonicalEvent {
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id: EventId::from_raw(t_ns),
            instrument: InstrumentId::from_u128(1),
            venue: Venue::Fixture,
            sequence_id: SequenceId::from_raw(t_ns),
            exchange_ts: ExchangeTs::new(UnixNanos::from_nanos(t_ns)),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(t_ns)),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(t_ns)),
            venue_status: VenueStatus::Normal,
            quality: DataQuality::default(),
            payload: argus_schema::EventPayload::Trade(TradePayload {
                price: PriceTicks::from_ticks(price_ticks, ts()),
                qty: QtyLots::from_lots(qty_lots, lot()),
                aggressor: side,
                trade_id: TradeId::new(format!("t-{t_ns}")),
                aggressor_is_maker: false,
            }),
        }
    }

    #[test]
    fn aligns_correctly() {
        let tf = Timeframe::ONE_MINUTE;
        // 90 segundos => bucket de 60-120s.
        let ts = UnixNanos::from_nanos(90 * 1_000_000_000);
        assert_eq!(tf.align(ts).as_nanos(), 60 * 1_000_000_000);
    }

    #[test]
    fn builds_single_candle() {
        let mut b = CandleBuilder::new(Timeframe::ONE_SECOND, ts(), lot());
        let e1 = trade_at(100_000_000, 5_000_000, 100, Side::Buy);
        let e2 = trade_at(500_000_000, 5_001_000, 200, Side::Sell);
        let e3 = trade_at(700_000_000, 4_999_000, 50, Side::Buy);
        assert!(b.on_event(&e1).is_none());
        assert!(b.on_event(&e2).is_none());
        assert!(b.on_event(&e3).is_none());
        let c = b.flush().unwrap();
        assert_eq!(c.trade_count, 3);
        assert_eq!(c.open.ticks, 5_000_000);
        assert_eq!(c.high.ticks, 5_001_000);
        assert_eq!(c.low.ticks, 4_999_000);
        assert_eq!(c.close.ticks, 4_999_000);
        assert_eq!(c.volume.lots, 350);
        assert_eq!(c.volume_buy.lots, 150);
        assert_eq!(c.volume_sell.lots, 200);
    }

    #[test]
    fn closes_candle_when_window_advances() {
        let mut b = CandleBuilder::new(Timeframe::ONE_SECOND, ts(), lot());
        let e1 = trade_at(100_000_000, 5_000_000, 100, Side::Buy);
        let e2 = trade_at(2_500_000_000, 5_100_000, 200, Side::Sell); // próximo segundo
        assert!(b.on_event(&e1).is_none());
        let closed = b.on_event(&e2).expect("candle anterior fechado");
        assert_eq!(closed.trade_count, 1);
        assert_eq!(closed.close.ticks, 5_000_000);
    }
}
