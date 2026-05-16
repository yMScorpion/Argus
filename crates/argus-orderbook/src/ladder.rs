use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use slab::Slab;

use argus_core_types::Side;
use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};

/// Nível de preço no orderbook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level {
    /// Preço em ticks.
    pub px_ticks: i128,
    /// Quantidade em lots.
    pub size_lots: i128,
    /// Última sequence que mexeu neste level (monotônico).
    pub last_update_seq: u64,
}

/// Side do book (cópia local para evitar conflito de import com schema).
fn cmp_for_side(side: Side, a: i128, b: i128) -> std::cmp::Ordering {
    match side {
        // Bids: best-first = decrescente.
        Side::Buy => b.cmp(&a),
        // Asks: best-first = crescente.
        Side::Sell => a.cmp(&b),
    }
}

/// Ladder de preços para um lado do orderbook.
#[derive(Debug, Clone)]
pub struct PriceLadder {
    side: Side,
    tick_size: TickSize,
    lot_size: LotSize,
    /// Arena estável.
    levels: Slab<Level>,
    /// Index esparso: preço (em ticks) → slab key.
    index: HashMap<i128, usize>,
    /// Ordem best-first: para bids decrescente; para asks crescente.
    /// Mantido sincronizado com `index`. `Vec` aceitável para profundidades
    /// típicas em cripto (até alguns milhares de levels visíveis); para
    /// mercados super densos, swap para skip-list.
    sorted: Vec<(i128, usize)>,
}

impl PriceLadder {
    /// Constrói ladder vazio.
    pub fn new(side: Side, tick_size: TickSize, lot_size: LotSize) -> Self {
        Self {
            side,
            tick_size,
            lot_size,
            levels: Slab::with_capacity(256),
            index: HashMap::with_capacity(256),
            sorted: Vec::with_capacity(256),
        }
    }

    /// Limpa ladder (usado por snapshot).
    pub fn clear(&mut self) {
        self.levels.clear();
        self.index.clear();
        self.sorted.clear();
    }

    /// Aplica um update de level. `size_lots == 0` remove o level.
    ///
    /// Retorna `true` se houve mudança estrutural (insert ou delete) vs.
    /// apenas update de qty em level existente.
    pub fn apply(&mut self, px_ticks: i128, size_lots: i128, seq: u64) -> bool {
        if size_lots == 0 {
            // Remove se existir.
            if let Some(&key) = self.index.get(&px_ticks) {
                self.levels.remove(key);
                self.index.remove(&px_ticks);
                if let Some(pos) = self.sorted.iter().position(|(p, _)| *p == px_ticks) {
                    self.sorted.remove(pos);
                }
                return true;
            }
            return false;
        }
        // size > 0
        if let Some(&key) = self.index.get(&px_ticks) {
            // Update existing.
            let level = &mut self.levels[key];
            level.size_lots = size_lots;
            // Só aceita seq se monotônico (não retrocede).
            if seq > level.last_update_seq {
                level.last_update_seq = seq;
            }
            false
        } else {
            // Insert.
            let key = self.levels.insert(Level {
                px_ticks,
                size_lots,
                last_update_seq: seq,
            });
            self.index.insert(px_ticks, key);
            // Insere em sorted mantendo ordem.
            let side = self.side;
            let pos = self
                .sorted
                .binary_search_by(|(p, _)| cmp_for_side(side, *p, px_ticks))
                .unwrap_or_else(|p| p);
            self.sorted.insert(pos, (px_ticks, key));
            true
        }
    }

    /// Best price (best-first) ou None se vazio.
    pub fn best(&self) -> Option<&Level> {
        self.sorted.first().map(|(_, k)| &self.levels[*k])
    }

    /// Iterador best-first sobre levels.
    pub fn iter(&self) -> impl Iterator<Item = &Level> + '_ {
        self.sorted.iter().map(move |(_, k)| &self.levels[*k])
    }

    /// Número de levels.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Vazio.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Tick size associado.
    pub fn tick_size(&self) -> TickSize {
        self.tick_size
    }

    /// Lot size associado.
    pub fn lot_size(&self) -> LotSize {
        self.lot_size
    }

    /// Side.
    pub fn side(&self) -> Side {
        self.side
    }

    /// Best preço como `PriceTicks` (convenience).
    pub fn best_price(&self) -> Option<PriceTicks> {
        self.best().map(|l| PriceTicks::from_ticks(l.px_ticks, self.tick_size))
    }

    /// Best size como `QtyLots`.
    pub fn best_size(&self) -> Option<QtyLots> {
        self.best().map(|l| QtyLots::from_lots(l.size_lots, self.lot_size))
    }

    /// Verifica invariantes (custoso; use em debug build / property tests).
    #[doc(hidden)]
    pub fn assert_invariants(&self) {
        // 1. index.len() == sorted.len() == levels.len() ativos.
        assert_eq!(
            self.index.len(),
            self.sorted.len(),
            "index/sorted desincronizados"
        );
        let active_levels = self.levels.iter().count();
        assert_eq!(
            self.index.len(),
            active_levels,
            "index/slab desincronizados"
        );

        // 2. Sorted é ordenado best-first.
        for w in self.sorted.windows(2) {
            let order = cmp_for_side(self.side, w[0].0, w[1].0);
            assert_ne!(order, std::cmp::Ordering::Greater, "sorted desordenado");
        }

        // 3. Todos sizes > 0.
        for (_, key) in &self.sorted {
            assert!(self.levels[*key].size_lots > 0);
        }

        // 4. index e sorted batem nos mesmos keys.
        for (px, key) in &self.sorted {
            assert_eq!(self.index.get(px), Some(key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn bid_ladder() -> PriceLadder {
        PriceLadder::new(
            Side::Buy,
            TickSize::new(dec!(0.01)).unwrap(),
            LotSize::new(dec!(0.001)).unwrap(),
        )
    }

    fn ask_ladder() -> PriceLadder {
        PriceLadder::new(
            Side::Sell,
            TickSize::new(dec!(0.01)).unwrap(),
            LotSize::new(dec!(0.001)).unwrap(),
        )
    }

    #[test]
    fn bid_orders_descending() {
        let mut l = bid_ladder();
        l.apply(100, 50, 1);
        l.apply(102, 30, 2);
        l.apply(101, 20, 3);
        l.apply(99, 10, 4);
        l.assert_invariants();
        let prices: Vec<i128> = l.iter().map(|x| x.px_ticks).collect();
        assert_eq!(prices, vec![102, 101, 100, 99]);
    }

    #[test]
    fn ask_orders_ascending() {
        let mut l = ask_ladder();
        l.apply(100, 50, 1);
        l.apply(98, 30, 2);
        l.apply(99, 20, 3);
        l.apply(102, 10, 4);
        l.assert_invariants();
        let prices: Vec<i128> = l.iter().map(|x| x.px_ticks).collect();
        assert_eq!(prices, vec![98, 99, 100, 102]);
    }

    #[test]
    fn zero_size_removes_level() {
        let mut l = bid_ladder();
        l.apply(100, 50, 1);
        l.apply(101, 30, 2);
        assert_eq!(l.len(), 2);
        l.apply(100, 0, 3);
        l.assert_invariants();
        assert_eq!(l.len(), 1);
        assert_eq!(l.best().unwrap().px_ticks, 101);
    }

    #[test]
    fn update_existing_level() {
        let mut l = bid_ladder();
        l.apply(100, 50, 1);
        l.apply(100, 70, 2);
        l.assert_invariants();
        assert_eq!(l.len(), 1);
        assert_eq!(l.best().unwrap().size_lots, 70);
    }

    #[test]
    fn many_inserts_and_deletes() {
        let mut l = ask_ladder();
        for i in 0..1000 {
            l.apply(1000 + i, 10, i as u64);
        }
        l.assert_invariants();
        assert_eq!(l.len(), 1000);
        // Remove os 500 do meio.
        for i in 250..750 {
            l.apply(1000 + i, 0, (1000 + i) as u64);
        }
        l.assert_invariants();
        assert_eq!(l.len(), 500);
    }

    #[test]
    fn clear_resets() {
        let mut l = bid_ladder();
        l.apply(100, 50, 1);
        l.apply(101, 30, 2);
        l.clear();
        l.assert_invariants();
        assert!(l.is_empty());
        assert!(l.best().is_none());
    }
}
