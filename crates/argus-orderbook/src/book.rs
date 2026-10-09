use serde::{Deserialize, Serialize};

use argus_core_types::Side;
use argus_decimal::{LotSize, PriceTicks, TickSize};
use argus_errors::ArgusError;
use argus_schema::{BookDelta, BookSnapshot};

use crate::ladder::PriceLadder;

/// Estado do orderbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookStatus {
    /// Sem snapshot inicial.
    Empty,
    /// Snapshot aplicado; pronto para deltas.
    SnapshotLoaded,
    /// Operação normal (deltas chegando).
    Live,
    /// Gap de sequence detectado.
    GapDetected,
    /// Resync em andamento (snapshot REST requisitado).
    Resyncing,
    /// Stale: sem updates há muito tempo.
    Stale,
    /// Inválido: invariante violada.
    Invalid,
}

/// Erros do orderbook.
#[derive(Debug, thiserror::Error, Clone)]
pub enum OrderBookError {
    /// Delta com `from_seq` não bate com sequence atual.
    #[error("sequence gap: expected from_seq>={expected}, got {got}")]
    SequenceGap {
        /// Esperado.
        expected: u64,
        /// Obtido.
        got: u64,
    },
    /// Book em estado inválido para operação.
    #[error("book status invalid for op: {status:?}")]
    InvalidStatus {
        /// Status atual.
        status: BookStatus,
    },
    /// Tick size de level no payload não bate com o do book.
    #[error("mismatched tick size")]
    MismatchedTickSize,
}

impl From<OrderBookError> for ArgusError {
    fn from(e: OrderBookError) -> Self {
        ArgusError::Internal { message: e.to_string() }
    }
}

/// Orderbook L2 para um instrumento.
#[derive(Debug, Clone)]
pub struct OrderBook {
    /// Bids ladder.
    bids: PriceLadder,
    /// Asks ladder.
    asks: PriceLadder,
    /// Sequence id do último update aplicado.
    last_seq: u64,
    /// Estado do book.
    status: BookStatus,
    /// Tick size oficial do instrumento.
    tick_size: TickSize,
    /// Lot size oficial.
    #[allow(dead_code)] // Usado para criar deltas em fase 5+ replay/snapshot persist.
    lot_size: LotSize,
}

impl OrderBook {
    /// Constrói orderbook vazio.
    pub fn new(tick_size: TickSize, lot_size: LotSize) -> Self {
        Self {
            bids: PriceLadder::new(Side::Buy, tick_size, lot_size),
            asks: PriceLadder::new(Side::Sell, tick_size, lot_size),
            last_seq: 0,
            status: BookStatus::Empty,
            tick_size,
            lot_size,
        }
    }

    /// Bids ladder.
    pub fn bids(&self) -> &PriceLadder {
        &self.bids
    }

    /// Asks ladder.
    pub fn asks(&self) -> &PriceLadder {
        &self.asks
    }

    /// Status atual.
    pub fn status(&self) -> BookStatus {
        self.status
    }

    /// Sequence atual.
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Best bid.
    pub fn best_bid(&self) -> Option<PriceTicks> {
        self.bids.best_price()
    }

    /// Best ask.
    pub fn best_ask(&self) -> Option<PriceTicks> {
        self.asks.best_price()
    }

    /// Aplica snapshot. Substitui book inteiro (atomic do ponto de vista do
    /// consumer, dado que o snapshot é processado entre dois reads do book).
    pub fn apply_snapshot(&mut self, snap: &BookSnapshot) -> Result<(), OrderBookError> {
        self.bids.clear();
        self.asks.clear();
        for level in &snap.bids {
            if level.price.tick_size != self.tick_size {
                return Err(OrderBookError::MismatchedTickSize);
            }
            if level.qty.lots != 0 {
                self.bids.apply(level.price.ticks, level.qty.lots, snap.snapshot_id);
            }
        }
        for level in &snap.asks {
            if level.price.tick_size != self.tick_size {
                return Err(OrderBookError::MismatchedTickSize);
            }
            if level.qty.lots != 0 {
                self.asks.apply(level.price.ticks, level.qty.lots, snap.snapshot_id);
            }
        }
        self.last_seq = snap.snapshot_id;
        self.status = if self.bids.is_empty() && self.asks.is_empty() {
            BookStatus::Empty
        } else {
            BookStatus::SnapshotLoaded
        };

        #[cfg(debug_assertions)]
        self.assert_invariants();

        Ok(())
    }

    /// Aplica delta. Valida sequencing (from_seq deve bater com last_seq).
    pub fn apply_delta(&mut self, delta: &BookDelta) -> Result<(), OrderBookError> {
        match self.status {
            BookStatus::Empty => {
                // Não podemos aplicar delta sem snapshot.
                self.status = BookStatus::GapDetected;
                return Err(OrderBookError::InvalidStatus { status: BookStatus::Empty });
            }
            BookStatus::Invalid => {
                return Err(OrderBookError::InvalidStatus { status: BookStatus::Invalid });
            }
            _ => {}
        }

        // Política de sequencing: from_seq deve ser <= last_seq + 1.
        // Permite re-aplicação idempotente (same range) e tolera 1-step
        // overlap. Skip > 1 = gap.
        if delta.from_seq > self.last_seq.saturating_add(1) {
            self.status = BookStatus::GapDetected;
            return Err(OrderBookError::SequenceGap {
                expected: self.last_seq + 1,
                got: delta.from_seq,
            });
        }

        // Aplica.
        for level in &delta.bid_changes {
            if level.price.tick_size != self.tick_size {
                self.status = BookStatus::Invalid;
                return Err(OrderBookError::MismatchedTickSize);
            }
            self.bids.apply(level.price.ticks, level.qty.lots, delta.to_seq);
        }
        for level in &delta.ask_changes {
            if level.price.tick_size != self.tick_size {
                self.status = BookStatus::Invalid;
                return Err(OrderBookError::MismatchedTickSize);
            }
            self.asks.apply(level.price.ticks, level.qty.lots, delta.to_seq);
        }
        self.last_seq = delta.to_seq;
        self.status = BookStatus::Live;

        #[cfg(debug_assertions)]
        self.assert_invariants();

        Ok(())
    }

    /// Marca book como em resync (snapshot REST sendo aguardado).
    pub fn mark_resyncing(&mut self) {
        self.status = BookStatus::Resyncing;
    }

    /// Verifica invariantes (chamada em debug builds).
    #[doc(hidden)]
    pub fn assert_invariants(&self) {
        self.bids.assert_invariants();
        self.asks.assert_invariants();
        // best_bid < best_ask em estado Live.
        if matches!(self.status, BookStatus::Live | BookStatus::SnapshotLoaded) {
            if let (Some(b), Some(a)) = (self.bids.best(), self.asks.best()) {
                assert!(
                    b.px_ticks < a.px_ticks,
                    "best_bid {} >= best_ask {} em estado {:?}",
                    b.px_ticks,
                    a.px_ticks,
                    self.status
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_decimal::QtyLots;
    use argus_schema::BookLevel;
    use rust_decimal_macros::dec;
    use smallvec::smallvec;

    fn book() -> OrderBook {
        OrderBook::new(
            TickSize::new(dec!(0.01)).unwrap(),
            LotSize::new(dec!(0.001)).unwrap(),
        )
    }

    fn lvl(price_ticks: i128, qty_lots: i128) -> BookLevel {
        BookLevel {
            price: PriceTicks::from_ticks(price_ticks, TickSize::new(dec!(0.01)).unwrap()),
            qty: QtyLots::from_lots(qty_lots, LotSize::new(dec!(0.001)).unwrap()),
        }
    }

    #[test]
    fn snapshot_initializes_book() {
        let mut b = book();
        let snap = BookSnapshot {
            snapshot_id: 100,
            bids: smallvec![lvl(99, 10), lvl(98, 20)],
            asks: smallvec![lvl(101, 15), lvl(102, 25)],
        };
        b.apply_snapshot(&snap).unwrap();
        assert_eq!(b.best_bid().unwrap().ticks, 99);
        assert_eq!(b.best_ask().unwrap().ticks, 101);
        assert_eq!(b.last_seq(), 100);
        assert_eq!(b.status(), BookStatus::SnapshotLoaded);
    }

    #[test]
    fn delta_advances_book() {
        let mut b = book();
        let snap = BookSnapshot {
            snapshot_id: 100,
            bids: smallvec![lvl(99, 10), lvl(98, 20)],
            asks: smallvec![lvl(101, 15), lvl(102, 25)],
        };
        b.apply_snapshot(&snap).unwrap();
        let delta = BookDelta {
            from_seq: 100,
            to_seq: 101,
            bid_changes: smallvec![lvl(100, 5)], // new best bid
            ask_changes: smallvec![],
        };
        b.apply_delta(&delta).unwrap();
        assert_eq!(b.best_bid().unwrap().ticks, 100);
        assert_eq!(b.last_seq(), 101);
        assert_eq!(b.status(), BookStatus::Live);
    }

    #[test]
    fn sequence_gap_detected() {
        let mut b = book();
        let snap = BookSnapshot {
            snapshot_id: 100,
            bids: smallvec![lvl(99, 10)],
            asks: smallvec![lvl(101, 15)],
        };
        b.apply_snapshot(&snap).unwrap();
        // Pula sequence — from_seq = 200 quando esperamos <= 101.
        let delta = BookDelta {
            from_seq: 200,
            to_seq: 201,
            bid_changes: smallvec![lvl(100, 5)],
            ask_changes: smallvec![],
        };
        let r = b.apply_delta(&delta);
        assert!(matches!(r, Err(OrderBookError::SequenceGap { .. })));
        assert_eq!(b.status(), BookStatus::GapDetected);
    }

    #[test]
    fn cant_apply_delta_to_empty_book() {
        let mut b = book();
        let delta = BookDelta {
            from_seq: 1,
            to_seq: 2,
            bid_changes: smallvec![lvl(99, 10)],
            ask_changes: smallvec![],
        };
        let r = b.apply_delta(&delta);
        assert!(matches!(r, Err(OrderBookError::InvalidStatus { .. })));
    }

    #[test]
    fn zero_qty_removes_level_via_delta() {
        let mut b = book();
        let snap = BookSnapshot {
            snapshot_id: 100,
            bids: smallvec![lvl(99, 10), lvl(98, 20)],
            asks: smallvec![lvl(101, 15)],
        };
        b.apply_snapshot(&snap).unwrap();
        let delta = BookDelta {
            from_seq: 100,
            to_seq: 101,
            bid_changes: smallvec![lvl(99, 0)],
            ask_changes: smallvec![],
        };
        b.apply_delta(&delta).unwrap();
        assert_eq!(b.best_bid().unwrap().ticks, 98);
        assert_eq!(b.bids().len(), 1);
    }

    #[test]
    fn snapshot_resyncs_book() {
        let mut b = book();
        let snap1 = BookSnapshot {
            snapshot_id: 100,
            bids: smallvec![lvl(99, 10)],
            asks: smallvec![lvl(101, 15)],
        };
        b.apply_snapshot(&snap1).unwrap();

        // Re-snapshot, book inteiro substituído.
        let snap2 = BookSnapshot {
            snapshot_id: 200,
            bids: smallvec![lvl(105, 5)],
            asks: smallvec![lvl(110, 8)],
        };
        b.apply_snapshot(&snap2).unwrap();
        assert_eq!(b.best_bid().unwrap().ticks, 105);
        assert_eq!(b.best_ask().unwrap().ticks, 110);
        assert_eq!(b.last_seq(), 200);
    }
}
