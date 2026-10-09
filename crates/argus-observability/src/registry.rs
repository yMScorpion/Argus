//! Registry de métricas com labels canônicos.
//!
//! Cada métrica é identificada por (nome, labels). Labels são sorted-pairs
//! (BTreeMap-like) para garantir ordering estável em Prometheus exposition.
//!
//! Suporta:
//! - Counter (monotônico)
//! - Gauge (set/get)
//! - Histogram (P50/P95/P99 via [`argus_metrics::Histogram`])

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::RwLock;

use argus_metrics::{Counter, Gauge, Histogram};

/// Label canônico (nome, valor).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Label {
    /// Nome (ex: "venue", "symbol", "channel").
    pub key: String,
    /// Valor (ex: "binance", "BTCUSDT", "depth").
    pub value: String,
}

impl Label {
    /// Constrói label.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self { key: key.into(), value: value.into() }
    }
}

/// Conjunto de labels associados a uma métrica.
///
/// `Vec` ordenado para garantir ordering estável (canonical Prometheus output).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LabelSet {
    pairs: Vec<Label>,
}

impl LabelSet {
    /// Vazio.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Constrói com lista.
    pub fn new(mut labels: Vec<Label>) -> Self {
        labels.sort();
        labels.dedup_by(|a, b| a.key == b.key);
        Self { pairs: labels }
    }

    /// Adiciona label (re-sort).
    pub fn add(&mut self, label: Label) {
        // Remove duplicate.
        self.pairs.retain(|p| p.key != label.key);
        self.pairs.push(label);
        self.pairs.sort();
    }

    /// Itera.
    pub fn iter(&self) -> impl Iterator<Item = &Label> {
        self.pairs.iter()
    }

    /// Quantas labels.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Vazio?
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// Kind de métrica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    /// Counter monotônico.
    Counter,
    /// Gauge (set/get).
    Gauge,
    /// Histogram (latency, sizes).
    Histogram,
}

/// Família de métricas (mesmo nome, várias label combinations).
#[derive(Debug)]
struct MetricFamily {
    kind: MetricKind,
    help: String,
    counters: BTreeMap<LabelSet, Arc<Counter>>,
    gauges: BTreeMap<LabelSet, Arc<Gauge>>,
    histograms: BTreeMap<LabelSet, Arc<Histogram>>,
}

impl MetricFamily {
    fn new(kind: MetricKind, help: String) -> Self {
        Self {
            kind,
            help,
            counters: BTreeMap::new(),
            gauges: BTreeMap::new(),
            histograms: BTreeMap::new(),
        }
    }
}

/// Registry global de métricas.
///
/// Thread-safe via `RwLock`. Métricas são `Arc` — coletadas a qualquer hora
/// sem bloquear hot path em writers (cada Counter/Gauge/Histogram já é
/// atomic ou usa lock fino).
#[derive(Debug, Default)]
pub struct MetricsRegistry {
    families: RwLock<BTreeMap<String, MetricFamily>>,
}

impl MetricsRegistry {
    /// Constrói registry vazio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registra (ou retorna existente) um counter.
    pub fn counter(&self, name: &str, help: &str, labels: LabelSet) -> Arc<Counter> {
        let mut families = self.families.write();
        let family = families
            .entry(name.to_string())
            .or_insert_with(|| MetricFamily::new(MetricKind::Counter, help.to_string()));
        assert_eq!(family.kind, MetricKind::Counter, "metric kind mismatch for {name}");
        family
            .counters
            .entry(labels)
            .or_insert_with(|| Arc::new(Counter::new()))
            .clone()
    }

    /// Registra um gauge.
    pub fn gauge(&self, name: &str, help: &str, labels: LabelSet) -> Arc<Gauge> {
        let mut families = self.families.write();
        let family = families
            .entry(name.to_string())
            .or_insert_with(|| MetricFamily::new(MetricKind::Gauge, help.to_string()));
        assert_eq!(family.kind, MetricKind::Gauge, "metric kind mismatch for {name}");
        family
            .gauges
            .entry(labels)
            .or_insert_with(|| Arc::new(Gauge::new()))
            .clone()
    }

    /// Registra um histogram.
    pub fn histogram(&self, name: &str, help: &str, labels: LabelSet) -> Arc<Histogram> {
        let mut families = self.families.write();
        let family = families
            .entry(name.to_string())
            .or_insert_with(|| MetricFamily::new(MetricKind::Histogram, help.to_string()));
        assert_eq!(family.kind, MetricKind::Histogram, "metric kind mismatch for {name}");
        family
            .histograms
            .entry(labels)
            .or_insert_with(|| Arc::new(Histogram::new()))
            .clone()
    }

    /// Itera famílias para export.
    pub(crate) fn for_each_family(
        &self,
        mut visitor: impl FnMut(&str, MetricKind, &str, &MetricFamilyView<'_>),
    ) {
        let families = self.families.read();
        for (name, family) in families.iter() {
            let view = MetricFamilyView { family };
            visitor(name, family.kind, &family.help, &view);
        }
    }
}

/// View imutável de uma família de métricas, usado pelo exporter.
pub(crate) struct MetricFamilyView<'a> {
    family: &'a MetricFamily,
}

impl<'a> MetricFamilyView<'a> {
    pub(crate) fn counters(&self) -> impl Iterator<Item = (&LabelSet, u64)> + '_ {
        self.family.counters.iter().map(|(ls, c)| (ls, c.get()))
    }

    pub(crate) fn gauges(&self) -> impl Iterator<Item = (&LabelSet, u64)> + '_ {
        self.family.gauges.iter().map(|(ls, g)| (ls, g.get()))
    }

    pub(crate) fn histograms(&self) -> impl Iterator<Item = (&LabelSet, argus_metrics::HistogramSnapshot)> + '_ {
        self.family.histograms.iter().map(|(ls, h)| (ls, h.snapshot()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_set_is_sorted() {
        let mut ls = LabelSet::empty();
        ls.add(Label::new("z", "zz"));
        ls.add(Label::new("a", "aa"));
        ls.add(Label::new("m", "mm"));
        let collected: Vec<_> = ls.iter().map(|l| l.key.clone()).collect();
        assert_eq!(collected, vec!["a", "m", "z"]);
    }

    #[test]
    fn label_set_deduplicates_keys() {
        let mut ls = LabelSet::empty();
        ls.add(Label::new("a", "first"));
        ls.add(Label::new("a", "second"));
        assert_eq!(ls.len(), 1);
        assert_eq!(ls.iter().next().unwrap().value, "second");
    }

    #[test]
    fn counter_registration_is_idempotent() {
        let r = MetricsRegistry::new();
        let labels = LabelSet::new(vec![Label::new("venue", "binance")]);
        let c1 = r.counter("events_total", "Total events", labels.clone());
        c1.inc(5);
        let c2 = r.counter("events_total", "Total events", labels);
        assert_eq!(c2.get(), 5, "same labels = same counter");
    }

    #[test]
    fn different_labels_are_different_metrics() {
        let r = MetricsRegistry::new();
        let c1 = r.counter(
            "events_total",
            "help",
            LabelSet::new(vec![Label::new("venue", "binance")]),
        );
        c1.inc(10);
        let c2 = r.counter(
            "events_total",
            "help",
            LabelSet::new(vec![Label::new("venue", "bybit")]),
        );
        c2.inc(3);
        // Distinct counters.
        assert_eq!(c1.get(), 10);
        assert_eq!(c2.get(), 3);
    }

    #[test]
    #[should_panic(expected = "metric kind mismatch")]
    fn mismatched_kinds_panic() {
        let r = MetricsRegistry::new();
        let _ = r.counter("name", "help", LabelSet::empty());
        let _ = r.gauge("name", "help", LabelSet::empty());
    }
}
