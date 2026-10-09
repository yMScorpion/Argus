//! Helpers de trace para propagação cross-process.
//!
//! Cada hot-path importante leva um `trace_id` desde a recepção do WS frame
//! até o ACK da venue (no caso de execução). Em fase 7+ esse trace flui
//! via OTLP exporter; nesta versão temos só os primitivos.

use argus_core_types::TraceId;
use argus_time::MonotonicNs;

/// Marco temporal de um stage do hot path.
#[derive(Debug, Clone, Copy)]
pub struct TraceStage {
    /// Nome canônico do stage.
    pub name: &'static str,
    /// Quando começou.
    pub start_ns: MonotonicNs,
    /// Quando terminou.
    pub end_ns: MonotonicNs,
}

impl TraceStage {
    /// Duração em nanos.
    pub fn duration_ns(&self) -> u64 {
        self.end_ns.as_nanos().saturating_sub(self.start_ns.as_nanos())
    }
}

/// Coleção de stages de um único trace_id.
///
/// Não é lock-free; uso esperado é por thread/task com handoff explícito.
#[derive(Debug, Clone, Default)]
pub struct Trace {
    /// ID do trace.
    pub trace_id: TraceId,
    /// Stages em ordem de start.
    pub stages: Vec<TraceStage>,
}

impl Trace {
    /// Constrói trace novo.
    pub fn new(trace_id: TraceId) -> Self {
        Self { trace_id, stages: Vec::new() }
    }

    /// Registra um stage completo.
    pub fn record(&mut self, name: &'static str, start_ns: MonotonicNs, end_ns: MonotonicNs) {
        self.stages.push(TraceStage { name, start_ns, end_ns });
    }

    /// Encontra duração total (último end - primeiro start). 0 se vazio.
    pub fn total_duration_ns(&self) -> u64 {
        let first = self.stages.iter().min_by_key(|s| s.start_ns.as_nanos());
        let last = self.stages.iter().max_by_key(|s| s.end_ns.as_nanos());
        match (first, last) {
            (Some(f), Some(l)) => l.end_ns.as_nanos().saturating_sub(f.start_ns.as_nanos()),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_duration_basic() {
        let st = TraceStage {
            name: "decode",
            start_ns: MonotonicNs::from_nanos(100),
            end_ns: MonotonicNs::from_nanos(450),
        };
        assert_eq!(st.duration_ns(), 350);
    }

    #[test]
    fn trace_total_duration_spans_stages() {
        let mut t = Trace::new(TraceId::from_u128(1));
        t.record(
            "decode",
            MonotonicNs::from_nanos(100),
            MonotonicNs::from_nanos(200),
        );
        t.record(
            "normalize",
            MonotonicNs::from_nanos(200),
            MonotonicNs::from_nanos(350),
        );
        t.record(
            "publish",
            MonotonicNs::from_nanos(350),
            MonotonicNs::from_nanos(380),
        );
        assert_eq!(t.total_duration_ns(), 280);
    }

    #[test]
    fn empty_trace_returns_zero_duration() {
        let t = Trace::new(TraceId::from_u128(1));
        assert_eq!(t.total_duration_ns(), 0);
    }
}
