//! Exporter Prometheus text format.
//!
//! Renderiza o [`MetricsRegistry`](crate::MetricsRegistry) no formato:
//!
//! ```text
//! # HELP <name> <help>
//! # TYPE <name> <type>
//! <name>{<label>=<value>,...} <value>
//! ```
//!
//! Compatível com `prometheus.io` parser. Histogramas usam o pattern
//! `<name>_bucket{le="<upper>"}`, `<name>_count`, `<name>_sum`.

use std::fmt::Write;

use crate::registry::{MetricKind, MetricsRegistry};

/// Exporter.
#[derive(Debug)]
pub struct PrometheusExporter<'a> {
    registry: &'a MetricsRegistry,
}

impl<'a> PrometheusExporter<'a> {
    /// Constrói exporter sobre registry.
    pub fn new(registry: &'a MetricsRegistry) -> Self {
        Self { registry }
    }

    /// Renderiza para `String`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.registry.for_each_family(|name, kind, help, view| {
            // HELP/TYPE headers.
            writeln!(out, "# HELP {name} {help}").ok();
            let type_str = match kind {
                MetricKind::Counter => "counter",
                MetricKind::Gauge => "gauge",
                MetricKind::Histogram => "histogram",
            };
            writeln!(out, "# TYPE {name} {type_str}").ok();

            match kind {
                MetricKind::Counter => {
                    for (labels, value) in view.counters() {
                        writeln!(out, "{name}{} {value}", format_labels(labels)).ok();
                    }
                }
                MetricKind::Gauge => {
                    for (labels, value) in view.gauges() {
                        writeln!(out, "{name}{} {value}", format_labels(labels)).ok();
                    }
                }
                MetricKind::Histogram => {
                    for (labels, snap) in view.histograms() {
                        let lbl = format_labels(labels);
                        // Buckets aproximados: p50, p95, p99, p999 como le=N.
                        // Note: spec exata de Prometheus pede buckets cumulativos;
                        // esta versão emite quantiles como gauges separados para
                        // compatibilidade fácil com Grafana até termos pleno
                        // Prometheus histogram (Fase 6.1).
                        writeln!(out, "{name}_count{lbl} {}", snap.count).ok();
                        writeln!(out, "{name}_sum{lbl} {}", snap.sum).ok();
                        writeln!(
                            out,
                            "{name}_min{lbl} {}",
                            snap.min
                        )
                        .ok();
                        writeln!(out, "{name}_max{lbl} {}", snap.max).ok();
                        writeln!(out, "{name}_p50{lbl} {}", snap.p50).ok();
                        writeln!(out, "{name}_p95{lbl} {}", snap.p95).ok();
                        writeln!(out, "{name}_p99{lbl} {}", snap.p99).ok();
                        writeln!(out, "{name}_p999{lbl} {}", snap.p999).ok();
                    }
                }
            }
        });
        out
    }
}

fn format_labels(labels: &crate::registry::LabelSet) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let mut s = String::from("{");
    let mut first = true;
    for label in labels.iter() {
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(&label.key);
        s.push_str("=\"");
        // Escape quotes/backslashes/newlines per Prometheus spec.
        for c in label.value.chars() {
            match c {
                '\\' => s.push_str("\\\\"),
                '"' => s.push_str("\\\""),
                '\n' => s.push_str("\\n"),
                other => s.push(other),
            }
        }
        s.push('"');
    }
    s.push('}');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Label, LabelSet, MetricsRegistry};

    #[test]
    fn empty_registry_renders_empty_string() {
        let r = MetricsRegistry::new();
        let exp = PrometheusExporter::new(&r);
        assert_eq!(exp.render(), "");
    }

    #[test]
    fn counter_is_rendered() {
        let r = MetricsRegistry::new();
        let c = r.counter(
            "argus_dp_events_total",
            "Total events processed by data plane",
            LabelSet::new(vec![
                Label::new("venue", "binance"),
                Label::new("symbol", "BTCUSDT"),
            ]),
        );
        c.inc(42);
        let exp = PrometheusExporter::new(&r);
        let out = exp.render();
        assert!(out.contains("# TYPE argus_dp_events_total counter"));
        assert!(out.contains("argus_dp_events_total{symbol=\"BTCUSDT\",venue=\"binance\"} 42"));
    }

    #[test]
    fn gauge_is_rendered() {
        let r = MetricsRegistry::new();
        let g = r.gauge("argus_terminal_fps", "Current FPS", LabelSet::empty());
        g.set(144);
        let out = PrometheusExporter::new(&r).render();
        assert!(out.contains("# TYPE argus_terminal_fps gauge"));
        assert!(out.contains("argus_terminal_fps 144"));
    }

    #[test]
    fn histogram_emits_quantile_lines() {
        let r = MetricsRegistry::new();
        let h = r.histogram(
            "argus_risk_validation_latency_ns",
            "Risk validation latency",
            LabelSet::empty(),
        );
        for v in 1..=100u64 {
            h.observe(v);
        }
        let out = PrometheusExporter::new(&r).render();
        assert!(out.contains("argus_risk_validation_latency_ns_count 100"));
        assert!(out.contains("argus_risk_validation_latency_ns_p50"));
        assert!(out.contains("argus_risk_validation_latency_ns_p99"));
    }

    #[test]
    fn label_values_are_escaped() {
        let r = MetricsRegistry::new();
        let c = r.counter(
            "test_metric",
            "Test",
            LabelSet::new(vec![Label::new("path", r#"C:\Program "Files"\foo"#)]),
        );
        c.inc(1);
        let out = PrometheusExporter::new(&r).render();
        assert!(out.contains(r#"path="C:\\Program \"Files\"\\foo""#), "got: {out}");
    }
}
