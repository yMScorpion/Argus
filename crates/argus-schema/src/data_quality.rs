use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use argus_capability::Confidence;

/// Flag pontual de qualidade aplicado a um evento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum QualityFlag {
    /// Sequence id pulou; gap detected.
    SequenceGap,
    /// Evento chegou fora de ordem em relação a exchange_ts.
    OutOfOrder,
    /// `exchange_ts` muito antigo comparado a `recv_ts` (stale).
    Stale,
    /// Snapshot inicial; book ainda não totalmente reconstruído.
    Initialization,
    /// Snapshot reaplicado; deltas anteriores invalidados.
    Resync,
    /// Reconnect recente; possível buffer perdido.
    PostReconnect,
    /// Aggressor inferido (não declarado pela venue).
    InferredAggressor,
    /// Clock skew significativo detectado para esta venue.
    ClockSkewDetected,
    /// Evento late (chegou depois do watermark).
    LateEvent,
    /// Storage atrasado; evento ainda não durável.
    StorageLagged,
    /// Modo degraded (algum subsystem comprometido).
    Degraded,
}

/// Estado de qualidade anexado a cada `CanonicalEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataQuality {
    /// Confidence 0..=100 do evento + canal no momento.
    pub confidence: Confidence,
    /// Flags ativos. `SmallVec` evita alocação no caso comum (zero ou um).
    pub flags: SmallVec<[QualityFlag; 4]>,
    /// Source latency em microssegundos (recv_ts - exchange_ts).
    pub source_latency_us: u32,
}

impl Default for DataQuality {
    fn default() -> Self {
        Self {
            confidence: Confidence::high(),
            flags: SmallVec::new(),
            source_latency_us: 0,
        }
    }
}

impl DataQuality {
    /// Marca uma flag (idempotente; não duplica).
    pub fn mark(&mut self, flag: QualityFlag) {
        if !self.flags.iter().any(|f| *f == flag) {
            self.flags.push(flag);
        }
    }

    /// Indica se algum flag crítico está presente.
    pub fn has_critical(&self) -> bool {
        self.flags.iter().any(|f| {
            matches!(
                f,
                QualityFlag::SequenceGap
                    | QualityFlag::Resync
                    | QualityFlag::Degraded
                    | QualityFlag::Stale
            )
        })
    }

    /// Computa confidence reduzido baseado em flags.
    pub fn effective_confidence(&self) -> Confidence {
        let mut score = self.confidence.score as i16;
        for f in &self.flags {
            score -= match f {
                QualityFlag::SequenceGap => 30,
                QualityFlag::Resync => 20,
                QualityFlag::Stale => 15,
                QualityFlag::OutOfOrder => 10,
                QualityFlag::Degraded => 15,
                QualityFlag::Initialization => 10,
                QualityFlag::PostReconnect => 5,
                QualityFlag::InferredAggressor => 5,
                QualityFlag::ClockSkewDetected => 5,
                QualityFlag::LateEvent => 10,
                QualityFlag::StorageLagged => 0, // não afeta consumo live
            };
        }
        Confidence::new(score.clamp(0, 100) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_is_idempotent() {
        let mut q = DataQuality::default();
        q.mark(QualityFlag::SequenceGap);
        q.mark(QualityFlag::SequenceGap);
        assert_eq!(q.flags.len(), 1);
    }

    #[test]
    fn effective_confidence_decreases_with_flags() {
        let mut q = DataQuality::default();
        let baseline = q.effective_confidence().score;
        q.mark(QualityFlag::SequenceGap);
        let with_gap = q.effective_confidence().score;
        assert!(with_gap < baseline);
    }

    #[test]
    fn critical_detection() {
        let mut q = DataQuality::default();
        assert!(!q.has_critical());
        q.mark(QualityFlag::InferredAggressor);
        assert!(!q.has_critical());
        q.mark(QualityFlag::SequenceGap);
        assert!(q.has_critical());
    }
}
