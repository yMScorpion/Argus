//! Engine de referência baseado em `BTreeMap`.
//!
//! Usado **somente em property tests** para detectar divergência da
//! implementação principal (`OrderBook`). Não otimizado para produção.

use std::collections::BTreeMap;

use argus_schema::{BookDelta, BookSnapshot};

/// Reference orderbook simples para comparação.
#[derive(Debug, Default, Clone)]
pub struct ReferenceBook {
    /// Bids: preço (i128 ticks) → qty (i128 lots). Reversed iter para best.
    pub bids: BTreeMap<i128, i128>,
    /// Asks: preço → qty. Forward iter para best.
    pub asks: BTreeMap<i128, i128>,
    /// Sequence.
    pub last_seq: u64,
}

impl ReferenceBook {
    /// Aplica snapshot.
    pub fn apply_snapshot(&mut self, snap: &BookSnapshot) {
        self.bids.clear();
        self.asks.clear();
        for l in &snap.bids {
            if l.qty.lots != 0 {
                self.bids.insert(l.price.ticks, l.qty.lots);
            }
        }
        for l in &snap.asks {
            if l.qty.lots != 0 {
                self.asks.insert(l.price.ticks, l.qty.lots);
            }
        }
        self.last_seq = snap.snapshot_id;
    }

    /// Aplica delta.
    pub fn apply_delta(&mut self, delta: &BookDelta) {
        for l in &delta.bid_changes {
            if l.qty.lots == 0 {
                self.bids.remove(&l.price.ticks);
            } else {
                self.bids.insert(l.price.ticks, l.qty.lots);
            }
        }
        for l in &delta.ask_changes {
            if l.qty.lots == 0 {
                self.asks.remove(&l.price.ticks);
            } else {
                self.asks.insert(l.price.ticks, l.qty.lots);
            }
        }
        self.last_seq = delta.to_seq;
    }

    /// Best bid (maior preço).
    pub fn best_bid(&self) -> Option<(i128, i128)> {
        self.bids.iter().next_back().map(|(p, q)| (*p, *q))
    }

    /// Best ask (menor preço).
    pub fn best_ask(&self) -> Option<(i128, i128)> {
        self.asks.iter().next().map(|(p, q)| (*p, *q))
    }
}
