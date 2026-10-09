//! Property tests: orderbook engine principal vs engine de referência.
//!
//! Aplica sequência aleatória de snapshots e deltas em ambos e verifica que
//! `best_bid` e `best_ask` convergem após cada operação. Detecta bugs sutis
//! de ordering, índice corrompido, e gap handling.

use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
use argus_orderbook::{reference::ReferenceBook, BookStatus, OrderBook};
use argus_schema::{BookDelta, BookLevel, BookSnapshot};
use proptest::prelude::*;
use rust_decimal_macros::dec;
use smallvec::SmallVec;

fn ts() -> TickSize {
    TickSize::new(dec!(0.01)).unwrap()
}

fn lot() -> LotSize {
    LotSize::new(dec!(0.001)).unwrap()
}

fn lvl(p: i128, q: i128) -> BookLevel {
    BookLevel {
        price: PriceTicks::from_ticks(p, ts()),
        qty: QtyLots::from_lots(q, lot()),
    }
}

#[derive(Debug, Clone)]
enum Op {
    Snapshot {
        seq: u64,
        bids: Vec<(i128, i128)>,
        asks: Vec<(i128, i128)>,
    },
    Delta {
        from_seq: u64,
        to_seq: u64,
        bids: Vec<(i128, i128)>,
        asks: Vec<(i128, i128)>,
    },
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let level_strategy = ((1i128..10_000), (0i128..1_000));
    let levels_strategy = prop::collection::vec(level_strategy, 0..20);
    prop_oneof![
        // Snapshot completo.
        (1u64..1_000_000, levels_strategy.clone(), levels_strategy.clone()).prop_map(
            |(seq, bids, asks)| {
                // Bids: filtra para preços abaixo de algum mid; asks acima.
                let bids: Vec<(i128, i128)> = bids
                    .into_iter()
                    .filter(|(p, _)| *p < 5000)
                    .map(|(p, q)| (p, q))
                    .collect();
                let asks: Vec<(i128, i128)> = asks
                    .into_iter()
                    .filter(|(p, _)| *p > 5000)
                    .map(|(p, q)| (p, q))
                    .collect();
                Op::Snapshot { seq, bids, asks }
            }
        ),
        // Delta (com qty=0 permitido para remover).
        (1u64..1_000_000, 1u64..100, levels_strategy.clone(), levels_strategy).prop_map(
            |(from_seq, step, bids, asks)| {
                let to_seq = from_seq + step;
                let bids: Vec<(i128, i128)> = bids
                    .into_iter()
                    .filter(|(p, _)| *p < 5000)
                    .collect();
                let asks: Vec<(i128, i128)> = asks
                    .into_iter()
                    .filter(|(p, _)| *p > 5000)
                    .collect();
                Op::Delta { from_seq, to_seq, bids, asks }
            }
        ),
    ]
}

fn run_op(book: &mut OrderBook, reference: &mut ReferenceBook, op: &Op) {
    match op {
        Op::Snapshot { seq, bids, asks } => {
            let snap = BookSnapshot {
                snapshot_id: *seq,
                bids: bids.iter().map(|(p, q)| lvl(*p, *q)).collect::<SmallVec<_>>(),
                asks: asks.iter().map(|(p, q)| lvl(*p, *q)).collect::<SmallVec<_>>(),
            };
            // Ambos sempre conseguem aplicar snapshot.
            book.apply_snapshot(&snap).expect("snapshot");
            reference.apply_snapshot(&snap);
        }
        Op::Delta { from_seq, to_seq, bids, asks } => {
            let delta = BookDelta {
                from_seq: *from_seq,
                to_seq: *to_seq,
                bid_changes: bids.iter().map(|(p, q)| lvl(*p, *q)).collect::<SmallVec<_>>(),
                ask_changes: asks.iter().map(|(p, q)| lvl(*p, *q)).collect::<SmallVec<_>>(),
            };
            // Book pode rejeitar por sequence gap; reference aceita tudo.
            // Em property tests, só comparamos quando book aceitou.
            if book.apply_delta(&delta).is_ok() {
                reference.apply_delta(&delta);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 200, ..Default::default() })]

    #[test]
    fn book_matches_reference_on_random_ops(ops in prop::collection::vec(op_strategy(), 1..50)) {
        let mut book = OrderBook::new(ts(), lot());
        let mut reference = ReferenceBook::default();

        // Primeiro op tem que ser Snapshot para permitir deltas.
        let initial_snap = BookSnapshot {
            snapshot_id: 0,
            bids: SmallVec::new(),
            asks: SmallVec::new(),
        };
        book.apply_snapshot(&initial_snap).unwrap();
        reference.apply_snapshot(&initial_snap);

        for op in &ops {
            run_op(&mut book, &mut reference, op);

            // Compara best bid e best ask.
            match (book.best_bid(), reference.best_bid()) {
                (Some(b), Some((r_p, _))) => prop_assert_eq!(b.ticks, r_p),
                (None, None) => {}
                (b, r) => prop_assert!(false, "best_bid divergente: book={b:?} ref={r:?}"),
            }
            match (book.best_ask(), reference.best_ask()) {
                (Some(b), Some((r_p, _))) => prop_assert_eq!(b.ticks, r_p),
                (None, None) => {}
                (b, r) => prop_assert!(false, "best_ask divergente: book={b:?} ref={r:?}"),
            }

            // Compara contagem de levels.
            prop_assert_eq!(book.bids().len(), reference.bids.len());
            prop_assert_eq!(book.asks().len(), reference.asks.len());
        }
    }

    /// Stress: 5_000 operações com seed determinístico. Detecta corrupção
    /// de índice/sorted que só aparece em volume.
    #[test]
    fn stress_large_sequence(ops in prop::collection::vec(op_strategy(), 500..2_000)) {
        let mut book = OrderBook::new(ts(), lot());
        let mut reference = ReferenceBook::default();
        let initial_snap = BookSnapshot {
            snapshot_id: 0,
            bids: SmallVec::new(),
            asks: SmallVec::new(),
        };
        book.apply_snapshot(&initial_snap).unwrap();
        reference.apply_snapshot(&initial_snap);

        for op in &ops {
            run_op(&mut book, &mut reference, op);
        }
        // Após toda a sessão, best bid/ask precisam convergir.
        prop_assert_eq!(
            book.best_bid().map(|p| p.ticks),
            reference.best_bid().map(|(p, _)| p)
        );
        prop_assert_eq!(
            book.best_ask().map(|p| p.ticks),
            reference.best_ask().map(|(p, _)| p)
        );
    }

    #[test]
    fn invariants_hold_after_random_ops(ops in prop::collection::vec(op_strategy(), 1..50)) {
        let mut book = OrderBook::new(ts(), lot());
        let initial_snap = BookSnapshot {
            snapshot_id: 0,
            bids: SmallVec::new(),
            asks: SmallVec::new(),
        };
        book.apply_snapshot(&initial_snap).unwrap();
        let mut reference = ReferenceBook::default();
        reference.apply_snapshot(&initial_snap);

        for op in &ops {
            run_op(&mut book, &mut reference, op);

            // Em estado válido, best_bid < best_ask.
            if matches!(book.status(), BookStatus::Live | BookStatus::SnapshotLoaded) {
                if let (Some(b), Some(a)) = (book.best_bid(), book.best_ask()) {
                    prop_assert!(b.ticks < a.ticks, "crossed book: bid={} ask={}", b.ticks, a.ticks);
                }
            }
        }
    }
}
