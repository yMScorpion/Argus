//! Provenance tracking: cada métrica carrega sua origem e qualidade.
//!
//! Princípio: nenhum dado exibido ao trader sem provenance acessível. Hover
//! sempre mostra: fonte, latência, frescor, gaps recentes, confidence.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use argus_capability::Confidence;
use argus_core_types::Venue;

/// Contribuição de uma venue específica para uma métrica derivada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VenueContribution {
    /// Venue.
    pub venue: Venue,
    /// Peso relativo (0..=10000 representando 0..=100%).
    pub weight_bps: u16,
    /// Latência mediana da venue rolling window (em ms).
    pub source_latency_ms_p50: u16,
    /// Houve gap recente neste source?
    pub recent_gap: bool,
}

/// Provenance de um valor calculado, exibido ou derivado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Composição cross-venue (ou single-venue).
    pub sources: SmallVec<[VenueContribution; 4]>,
    /// Confidence agregada (mínima de sources, ajustada por staleness).
    pub confidence: Confidence,
    /// Tempo desde o último update relevante (ms).
    pub age_ms: u32,
    /// Indica que dado é estimativa intraday (vs oficial T+1) — usado em
    /// ETF, on-chain finalized vs pending, etc.
    pub is_estimate: bool,
    /// Versão do algoritmo de cálculo (semver).
    pub algo_version: u16,
}

impl Provenance {
    /// Cria provenance de single source (venue única, confidence alta).
    pub fn single(venue: Venue, age_ms: u32, conf: Confidence) -> Self {
        let mut sources = SmallVec::new();
        sources.push(VenueContribution {
            venue,
            weight_bps: 10_000,
            source_latency_ms_p50: 0,
            recent_gap: false,
        });
        Self {
            sources,
            confidence: conf,
            age_ms,
            is_estimate: false,
            algo_version: 0,
        }
    }

    /// Combina múltiplas provenances (para métricas derivadas que dependem
    /// de várias). Confidence resultante é o **mínimo**; idade é o **máximo**;
    /// estimate é OR.
    pub fn combine(items: &[&Self]) -> Self {
        if items.is_empty() {
            return Self {
                sources: SmallVec::new(),
                confidence: Confidence::new(0),
                age_ms: u32::MAX,
                is_estimate: true,
                algo_version: 0,
            };
        }
        let mut sources: SmallVec<[VenueContribution; 4]> = SmallVec::new();
        for it in items {
            for s in &it.sources {
                if let Some(existing) = sources
                    .iter_mut()
                    .find(|c: &&mut VenueContribution| c.venue == s.venue)
                {
                    existing.weight_bps = existing.weight_bps.saturating_add(s.weight_bps);
                    existing.source_latency_ms_p50 =
                        existing.source_latency_ms_p50.max(s.source_latency_ms_p50);
                    existing.recent_gap |= s.recent_gap;
                } else {
                    sources.push(s.clone());
                }
            }
        }
        // Normaliza pesos para somar 10_000.
        let total: u32 = sources.iter().map(|c| c.weight_bps as u32).sum();
        if total > 0 {
            for c in &mut sources {
                c.weight_bps = ((c.weight_bps as u32 * 10_000) / total) as u16;
            }
        }
        let confidence = items
            .iter()
            .map(|it| it.confidence)
            .reduce(|a, b| a.min(b))
            .unwrap_or_else(|| Confidence::new(0));
        let age_ms = items.iter().map(|it| it.age_ms).max().unwrap_or(0);
        let is_estimate = items.iter().any(|it| it.is_estimate);
        let algo_version = items.iter().map(|it| it.algo_version).max().unwrap_or(0);
        Self { sources, confidence, age_ms, is_estimate, algo_version }
    }

    /// Indica se o dado é considerado "fresco" para janela dada.
    pub fn is_fresh(&self, max_age_ms: u32) -> bool {
        self.age_ms <= max_age_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_capability::Confidence;

    #[test]
    fn single_source_provenance() {
        let p = Provenance::single(Venue::BinanceSpot, 50, Confidence::high());
        assert_eq!(p.sources.len(), 1);
        assert_eq!(p.sources[0].weight_bps, 10_000);
    }

    #[test]
    fn combine_minimums_confidence() {
        let a = Provenance::single(Venue::BinanceSpot, 50, Confidence::high());
        let b = Provenance::single(Venue::BybitLinear, 100, Confidence::low());
        let combined = Provenance::combine(&[&a, &b]);
        assert_eq!(combined.confidence.score, Confidence::low().score);
        assert_eq!(combined.age_ms, 100);
        assert_eq!(combined.sources.len(), 2);
    }

    #[test]
    fn combine_normalizes_weights() {
        let a = Provenance::single(Venue::BinanceSpot, 50, Confidence::high());
        let b = Provenance::single(Venue::BybitLinear, 50, Confidence::high());
        let combined = Provenance::combine(&[&a, &b]);
        let total: u32 = combined.sources.iter().map(|c| c.weight_bps as u32).sum();
        assert_eq!(total, 10_000);
    }
}
