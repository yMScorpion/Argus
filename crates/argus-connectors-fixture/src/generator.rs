//! Gerador determinístico de eventos sintéticos.
//!
//! Implementa um PRNG simples (xorshift64*) para evitar dependência externa
//! e garantir determinismo cross-platform. Seeds idênticos produzem sequência
//! idêntica em qualquer máquina.

use smallvec::SmallVec;

use argus_core_types::{
    EventId, InstrumentId, SequenceId, Side, TradeId, Venue, VenueStatus,
};
use argus_decimal::{LotSize, PriceTicks, QtyLots, TickSize};
use argus_schema::{
    BookDelta, BookLevel, BookSnapshot, CanonicalEvent, DataQuality, EventPayload,
    TradePayload, SCHEMA_VERSION,
};
use argus_time::{ExchangeTs, ProcessTs, RecvTs, UnixNanos};

/// Configuração de cenário.
#[derive(Debug, Clone)]
pub struct FixtureScenario {
    /// Seed determinística.
    pub seed: u64,
    /// Instrumento.
    pub instrument: InstrumentId,
    /// Venue.
    pub venue: Venue,
    /// Tick size.
    pub tick_size: TickSize,
    /// Lot size.
    pub lot_size: LotSize,
    /// Mid price inicial em ticks.
    pub initial_mid_ticks: i128,
    /// Início (Unix nanos).
    pub start_ts: UnixNanos,
    /// Intervalo médio entre eventos (ns).
    pub avg_interval_ns: u64,
    /// Total de eventos a gerar.
    pub event_count: u64,
    /// Parâmetros do random walk para mid.
    pub random_walk: RandomWalkParams,
}

/// Parâmetros do random walk.
#[derive(Debug, Clone, Copy)]
pub struct RandomWalkParams {
    /// Step máximo em ticks por evento (random uniform ±step).
    pub max_step_ticks: i32,
    /// Drift (bias positivo/negativo por evento, escalado para ticks).
    pub drift_ticks_per_event: i32,
    /// Volatilidade do tamanho do spread em ticks.
    pub spread_ticks: i32,
}

impl Default for RandomWalkParams {
    fn default() -> Self {
        Self {
            max_step_ticks: 5,
            drift_ticks_per_event: 0,
            spread_ticks: 2,
        }
    }
}

/// Config do gerador (com fração de tipo de evento).
#[derive(Debug, Clone, Copy)]
pub struct GeneratorConfig {
    /// % de eventos que são trades vs deltas (0..=100).
    pub trade_pct: u8,
    /// Gera snapshot a cada N eventos.
    pub snapshot_every: u64,
    /// Inclui gaps de sequence ocasionais (testar gap detection).
    pub inject_gaps: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            trade_pct: 50,
            snapshot_every: 1000,
            inject_gaps: false,
        }
    }
}

/// PRNG determinístico (xorshift64*).
///
/// Pequena, rápida, e suficiente para gerar fixtures não-críptográficos.
#[derive(Debug)]
struct Xorshift64Star {
    state: u64,
}

impl Xorshift64Star {
    fn new(seed: u64) -> Self {
        // Garante state != 0.
        Self { state: seed | 0x1 }
    }

    fn next(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Inteiro uniforme em `[-bound, bound]`.
    fn next_signed(&mut self, bound: i64) -> i64 {
        if bound == 0 {
            return 0;
        }
        let n = self.next();
        let range = (bound * 2 + 1) as u64;
        (n % range) as i64 - bound
    }

    /// Inteiro uniforme em `[0, bound)`.
    fn next_bounded(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        self.next() % bound
    }
}

/// Gerador iterativo de eventos sintéticos.
#[derive(Debug)]
pub struct FixtureGenerator {
    scenario: FixtureScenario,
    config: GeneratorConfig,
    rng: Xorshift64Star,
    current_mid_ticks: i128,
    current_ts: UnixNanos,
    next_seq: u64,
    next_event_id: u64,
    emitted: u64,
    /// Estado de book mantido em memória para gerar deltas consistentes.
    bids: SmallVec<[(i128, i128); 32]>,
    asks: SmallVec<[(i128, i128); 32]>,
    last_snapshot_id: u64,
}

impl FixtureGenerator {
    /// Constrói.
    pub fn new(scenario: FixtureScenario, config: GeneratorConfig) -> Self {
        let mid = scenario.initial_mid_ticks;
        let spread = scenario.random_walk.spread_ticks as i128;
        let mut bids = SmallVec::new();
        let mut asks = SmallVec::new();
        // Cria book inicial com 10 níveis cada lado.
        for i in 0..10 {
            bids.push((mid - spread / 2 - i as i128, 100 - i as i128 * 5));
            asks.push((mid + spread / 2 + i as i128, 100 - i as i128 * 5));
        }
        let rng = Xorshift64Star::new(scenario.seed);
        Self {
            current_mid_ticks: mid,
            current_ts: scenario.start_ts,
            next_seq: 1,
            next_event_id: 1,
            emitted: 0,
            bids,
            asks,
            last_snapshot_id: 0,
            scenario,
            config,
            rng,
        }
    }

    /// Total de eventos a serem emitidos.
    pub fn total_events(&self) -> u64 {
        self.scenario.event_count
    }

    fn advance_mid(&mut self) {
        let step = self.rng.next_signed(self.scenario.random_walk.max_step_ticks as i64);
        self.current_mid_ticks = self
            .current_mid_ticks
            .saturating_add(step as i128)
            .saturating_add(self.scenario.random_walk.drift_ticks_per_event as i128);
        // Mantém positivo.
        if self.current_mid_ticks < 100 {
            self.current_mid_ticks = 100;
        }
    }

    fn advance_ts(&mut self) {
        // Jitter ±50% do avg.
        let jitter = self.rng.next_signed((self.scenario.avg_interval_ns / 2) as i64);
        let next_ns = self
            .scenario
            .avg_interval_ns
            .saturating_add(jitter.max(0) as u64);
        self.current_ts = UnixNanos::from_nanos(self.current_ts.as_nanos().saturating_add(next_ns));
    }

    fn make_event(&mut self, payload: EventPayload) -> CanonicalEvent {
        let event_id = EventId::from_raw(self.next_event_id);
        self.next_event_id += 1;
        let seq = SequenceId::from_raw(self.next_seq);
        self.next_seq = self
            .next_seq
            .checked_add(1)
            .expect("fixture seq overflow");
        if self.config.inject_gaps && self.rng.next_bounded(100) == 7 {
            // Pula uma sequence ocasionalmente.
            self.next_seq += 1;
        }
        CanonicalEvent {
            schema_version: SCHEMA_VERSION,
            event_id,
            instrument: self.scenario.instrument,
            venue: self.scenario.venue,
            sequence_id: seq,
            exchange_ts: ExchangeTs::new(self.current_ts),
            recv_ts: RecvTs::new(UnixNanos::from_nanos(
                self.current_ts.as_nanos().saturating_add(50_000),
            )),
            process_ts: ProcessTs::new(UnixNanos::from_nanos(
                self.current_ts.as_nanos().saturating_add(100_000),
            )),
            venue_status: VenueStatus::Normal,
            quality: DataQuality::default(),
            payload,
        }
    }

    fn gen_snapshot(&mut self) -> CanonicalEvent {
        self.last_snapshot_id = self.next_seq;
        let bids_payload: SmallVec<[BookLevel; 64]> = self
            .bids
            .iter()
            .map(|(p, q)| BookLevel {
                price: PriceTicks::from_ticks(*p, self.scenario.tick_size),
                qty: QtyLots::from_lots(*q, self.scenario.lot_size),
            })
            .collect();
        let asks_payload: SmallVec<[BookLevel; 64]> = self
            .asks
            .iter()
            .map(|(p, q)| BookLevel {
                price: PriceTicks::from_ticks(*p, self.scenario.tick_size),
                qty: QtyLots::from_lots(*q, self.scenario.lot_size),
            })
            .collect();
        let snap = BookSnapshot {
            snapshot_id: self.last_snapshot_id,
            bids: bids_payload,
            asks: asks_payload,
        };
        self.make_event(EventPayload::BookSnapshot(snap))
    }

    fn gen_delta(&mut self) -> CanonicalEvent {
        let from_seq = self.last_snapshot_id.max(self.next_seq.saturating_sub(1));
        // Modifica 1-3 níveis de cada lado.
        let n_bids = (self.rng.next_bounded(3) + 1) as usize;
        let n_asks = (self.rng.next_bounded(3) + 1) as usize;
        let mut bid_changes: SmallVec<[BookLevel; 8]> = SmallVec::new();
        let mut ask_changes: SmallVec<[BookLevel; 8]> = SmallVec::new();
        for _ in 0..n_bids {
            if self.bids.is_empty() {
                break;
            }
            let idx = self.rng.next_bounded(self.bids.len() as u64) as usize;
            let (p, q) = self.bids[idx];
            let new_q = (q + self.rng.next_signed(20) as i128).max(0);
            self.bids[idx] = (p, new_q);
            bid_changes.push(BookLevel {
                price: PriceTicks::from_ticks(p, self.scenario.tick_size),
                qty: QtyLots::from_lots(new_q, self.scenario.lot_size),
            });
        }
        for _ in 0..n_asks {
            if self.asks.is_empty() {
                break;
            }
            let idx = self.rng.next_bounded(self.asks.len() as u64) as usize;
            let (p, q) = self.asks[idx];
            let new_q = (q + self.rng.next_signed(20) as i128).max(0);
            self.asks[idx] = (p, new_q);
            ask_changes.push(BookLevel {
                price: PriceTicks::from_ticks(p, self.scenario.tick_size),
                qty: QtyLots::from_lots(new_q, self.scenario.lot_size),
            });
        }
        let to_seq = self.next_seq;
        let delta = BookDelta {
            from_seq,
            to_seq,
            bid_changes,
            ask_changes,
        };
        self.make_event(EventPayload::BookDelta(delta))
    }

    fn gen_trade(&mut self) -> CanonicalEvent {
        let side = if self.rng.next() & 1 == 0 { Side::Buy } else { Side::Sell };
        let price_offset = self.rng.next_signed(2) as i128;
        let price_ticks = self.current_mid_ticks + price_offset;
        let qty_lots = (self.rng.next_bounded(100) + 1) as i128;
        let trade_id = TradeId::new(format!("syn-{}", self.next_event_id));
        let trade = TradePayload {
            price: PriceTicks::from_ticks(price_ticks, self.scenario.tick_size),
            qty: QtyLots::from_lots(qty_lots, self.scenario.lot_size),
            aggressor: side,
            trade_id,
            aggressor_is_maker: false,
        };
        self.make_event(EventPayload::Trade(trade))
    }
}

impl Iterator for FixtureGenerator {
    type Item = CanonicalEvent;

    fn next(&mut self) -> Option<Self::Item> {
        if self.emitted >= self.scenario.event_count {
            return None;
        }
        self.advance_ts();
        self.advance_mid();

        // Primeiro evento sempre é snapshot.
        let ev = if self.emitted == 0 {
            self.gen_snapshot()
        } else if self.emitted % self.config.snapshot_every == 0 {
            self.gen_snapshot()
        } else {
            let r = self.rng.next_bounded(100);
            if r < self.config.trade_pct as u64 {
                self.gen_trade()
            } else {
                self.gen_delta()
            }
        };
        self.emitted += 1;
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn scenario(seed: u64, n: u64) -> FixtureScenario {
        FixtureScenario {
            seed,
            instrument: InstrumentId::from_u128(0xDEAD),
            venue: Venue::Fixture,
            tick_size: TickSize::new(dec!(0.01)).unwrap(),
            lot_size: LotSize::new(dec!(0.001)).unwrap(),
            initial_mid_ticks: 5_000_000,
            start_ts: UnixNanos::from_nanos(1_700_000_000_000_000_000),
            avg_interval_ns: 1_000_000, // 1ms entre eventos
            event_count: n,
            random_walk: RandomWalkParams::default(),
        }
    }

    #[test]
    fn deterministic_same_seed_same_output() {
        let gen1 = FixtureGenerator::new(scenario(42, 100), GeneratorConfig::default());
        let gen2 = FixtureGenerator::new(scenario(42, 100), GeneratorConfig::default());
        let v1: Vec<_> = gen1.collect();
        let v2: Vec<_> = gen2.collect();
        assert_eq!(v1.len(), 100);
        assert_eq!(v1, v2);
    }

    #[test]
    fn different_seed_different_output() {
        let gen1: Vec<_> =
            FixtureGenerator::new(scenario(1, 100), GeneratorConfig::default()).collect();
        let gen2: Vec<_> =
            FixtureGenerator::new(scenario(2, 100), GeneratorConfig::default()).collect();
        assert_ne!(gen1, gen2);
    }

    #[test]
    fn produces_expected_event_count() {
        let n = 200u64;
        let gen = FixtureGenerator::new(scenario(7, n), GeneratorConfig::default());
        let events: Vec<_> = gen.collect();
        assert_eq!(events.len() as u64, n);
    }

    #[test]
    fn first_event_is_snapshot() {
        let mut gen = FixtureGenerator::new(scenario(1, 5), GeneratorConfig::default());
        let first = gen.next().unwrap();
        assert!(matches!(first.payload, EventPayload::BookSnapshot(_)));
    }

    #[test]
    fn sequences_are_monotonic_without_gaps() {
        let cfg = GeneratorConfig {
            inject_gaps: false,
            ..Default::default()
        };
        let mut last_seq = 0u64;
        for ev in FixtureGenerator::new(scenario(3, 500), cfg) {
            assert!(ev.sequence_id.raw() > last_seq);
            last_seq = ev.sequence_id.raw();
        }
    }
}
