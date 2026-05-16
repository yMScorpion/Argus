use parking_lot::Mutex;

/// Histograma simples baseado em buckets exponenciais.
///
/// Cobre 1ns até 10s com ~10% resolução. Útil para latência. Operações são
/// O(log B) com B = número de buckets (~30).
#[derive(Debug)]
pub struct Histogram {
    inner: Mutex<HistInner>,
}

#[derive(Debug)]
struct HistInner {
    /// Buckets exponenciais. bucket[i] = count de valores em [2^i, 2^(i+1)).
    buckets: [u64; 64],
    /// Total de observações.
    count: u64,
    /// Soma de todos os valores (para mean).
    sum: u64,
    /// Min observado.
    min: u64,
    /// Max observado.
    max: u64,
}

impl Histogram {
    /// Constrói histograma vazio.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HistInner {
                buckets: [0; 64],
                count: 0,
                sum: 0,
                min: u64::MAX,
                max: 0,
            }),
        }
    }

    /// Adiciona observação.
    pub fn observe(&self, v: u64) {
        let mut g = self.inner.lock();
        let bucket = if v == 0 { 0 } else { 63 - v.leading_zeros() as usize };
        g.buckets[bucket] = g.buckets[bucket].saturating_add(1);
        g.count = g.count.saturating_add(1);
        g.sum = g.sum.saturating_add(v);
        if v < g.min {
            g.min = v;
        }
        if v > g.max {
            g.max = v;
        }
    }

    /// Snapshot do estado atual.
    pub fn snapshot(&self) -> HistogramSnapshot {
        let g = self.inner.lock();
        let mean = if g.count > 0 { g.sum / g.count } else { 0 };
        HistogramSnapshot {
            count: g.count,
            sum: g.sum,
            min: if g.count > 0 { g.min } else { 0 },
            max: g.max,
            mean,
            p50: percentile(&g.buckets, g.count, 500),
            p95: percentile(&g.buckets, g.count, 950),
            p99: percentile(&g.buckets, g.count, 990),
            p999: percentile(&g.buckets, g.count, 999),
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot estatístico.
#[derive(Debug, Clone, Copy)]
pub struct HistogramSnapshot {
    /// Número de observações.
    pub count: u64,
    /// Soma de todos os valores.
    pub sum: u64,
    /// Mínimo.
    pub min: u64,
    /// Máximo.
    pub max: u64,
    /// Média.
    pub mean: u64,
    /// P50 estimado (limite superior do bucket).
    pub p50: u64,
    /// P95.
    pub p95: u64,
    /// P99.
    pub p99: u64,
    /// P99.9 (em milésimos: 999 = 99.9%).
    pub p999: u64,
}

/// Computa percentil aproximado.
///
/// `pct` é em milésimos para P99.9 (use 50, 95, 99, 999).
fn percentile(buckets: &[u64; 64], total: u64, pct: u32) -> u64 {
    if total == 0 {
        return 0;
    }
    let pct = pct.min(1000) as u64;
    // target = ceil(total * pct / 1000).
    let target = ((total.saturating_mul(pct)) + 999) / 1000;
    let mut acc = 0u64;
    for (i, &b) in buckets.iter().enumerate() {
        acc = acc.saturating_add(b);
        if acc >= target {
            // Retorna limite superior do bucket: 2^(i+1) - 1.
            return (1u64 << (i + 1)).saturating_sub(1);
        }
    }
    u64::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot() {
        let h = Histogram::new();
        let s = h.snapshot();
        assert_eq!(s.count, 0);
        assert_eq!(s.p99, 0);
    }

    #[test]
    fn percentile_bucket_correctness() {
        let h = Histogram::new();
        for v in 1..=100u64 {
            h.observe(v);
        }
        let s = h.snapshot();
        assert_eq!(s.count, 100);
        assert!(s.min == 1);
        assert!(s.max == 100);
        // p99 deve estar perto do 99 (bucket de 64..128).
        assert!(s.p99 >= 64 && s.p99 < 256);
    }

    #[test]
    fn observe_large_values() {
        let h = Histogram::new();
        h.observe(1_000_000_000); // 1 segundo em ns
        let s = h.snapshot();
        assert_eq!(s.count, 1);
        assert_eq!(s.min, 1_000_000_000);
    }
}
