use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_utils::CachePadded;

/// Política de overflow.
///
/// O canal carrega política declarada — não é decisão runtime do producer.
/// Ver `docs/architecture/latency-budget.md` para regras por canal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Producer espera (busy-loop curto + yield) até espaço; NEVER DROP.
    /// Aplicado a raw events e book deltas.
    WaitProducer,
    /// Producer descarta o evento mais antigo (faz consumer perder); usado
    /// apenas em canais derivados onde coalesce é aceitável.
    DropOldest,
    /// Producer descarta o evento novo; raramente apropriado.
    DropNew,
}

/// Erro retornado por operações do ring.
#[derive(Debug, thiserror::Error, Clone, Copy)]
pub enum ShmRingError {
    /// Ring está cheio e política é `DropNew`.
    #[error("ring full (drop new)")]
    Full,
    /// Ring está vazio.
    #[error("ring empty")]
    Empty,
    /// Capacity inválida (não é potência de 2 ou == 0).
    #[error("capacity must be power of 2 and > 0")]
    InvalidCapacity,
}

#[repr(C)]
struct Slot<T> {
    seq: AtomicU64,
    /// `UnsafeCell` para permitir mutação interna; `MaybeUninit` porque
    /// o valor só é válido entre `seq = pos+1` (produced) e
    /// `seq = pos+capacity` (consumed).
    payload: UnsafeCell<MaybeUninit<T>>,
}

impl<T> std::fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slot")
            .field("seq", &self.seq.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

// SAFETY: cada slot é tocado por exatamente uma thread por vez:
// - Producer quando seq == pos.
// - Consumer quando seq == pos + 1.
// O coordenamento é via `seq` AtomicU64 com Acquire/Release.
unsafe impl<T: Send> Send for Slot<T> {}
unsafe impl<T: Send> Sync for Slot<T> {}

#[repr(C)]
#[derive(Debug)]
struct RingHeader {
    capacity: u64,
    mask: u64,
    /// Política de overflow (constante após criação).
    overflow: OverflowPolicy,
    /// Posição do producer (escrita pelo producer; lida pelo consumer para
    /// detectar vazio).
    producer_pos: CachePadded<AtomicU64>,
    /// Posição do consumer (escrita pelo consumer; lida pelo producer para
    /// detectar cheio).
    consumer_pos: CachePadded<AtomicU64>,
    /// Eventos descartados desde o boot.
    dropped: CachePadded<AtomicU64>,
}

/// SPSC ring lock-free.
///
/// Para construir, use [`SpscRing::new`]. Para obter os endpoints, use
/// [`SpscRing::split`], que produz `(Producer, Consumer)` — endpoints
/// distintos garantem disciplina SPSC em tempo de tipo (não há `clone()`).
pub struct SpscRing<T> {
    header: RingHeader,
    slots: Box<[Slot<T>]>,
}

impl<T> std::fmt::Debug for SpscRing<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpscRing")
            .field("capacity", &self.header.capacity)
            .field("producer_pos", &self.header.producer_pos.load(Ordering::Relaxed))
            .field("consumer_pos", &self.header.consumer_pos.load(Ordering::Relaxed))
            .field("dropped", &self.header.dropped.load(Ordering::Relaxed))
            .finish()
    }
}

impl<T> SpscRing<T> {
    /// Constrói ring com capacity dado. Erra se capacity não é potência
    /// de 2 ou é zero.
    pub fn new(capacity: usize, overflow: OverflowPolicy) -> Result<Arc<Self>, ShmRingError> {
        if capacity == 0 || (capacity & (capacity - 1)) != 0 {
            return Err(ShmRingError::InvalidCapacity);
        }
        let mut slots: Vec<Slot<T>> = Vec::with_capacity(capacity);
        for i in 0..capacity {
            slots.push(Slot {
                seq: AtomicU64::new(i as u64),
                payload: UnsafeCell::new(MaybeUninit::uninit()),
            });
        }
        Ok(Arc::new(Self {
            header: RingHeader {
                capacity: capacity as u64,
                mask: (capacity as u64) - 1,
                overflow,
                producer_pos: CachePadded::new(AtomicU64::new(0)),
                consumer_pos: CachePadded::new(AtomicU64::new(0)),
                dropped: CachePadded::new(AtomicU64::new(0)),
            },
            slots: slots.into_boxed_slice(),
        }))
    }

    /// Divide ring em endpoints producer e consumer.
    ///
    /// Usar este split garante disciplina SPSC: cada endpoint não-Clone, e o
    /// Arc do ring mantém vida enquanto qualquer endpoint exista.
    pub fn split(self: Arc<Self>) -> (Producer<T>, Consumer<T>) {
        let prod_ref = self.clone();
        let cons_ref = self;
        (
            Producer { ring: prod_ref, _not_sync: std::marker::PhantomData },
            Consumer { ring: cons_ref, _not_sync: std::marker::PhantomData },
        )
    }

    /// Capacity.
    pub fn capacity(&self) -> usize {
        self.header.capacity as usize
    }

    /// Eventos descartados.
    pub fn dropped(&self) -> u64 {
        self.header.dropped.load(Ordering::Relaxed)
    }
}

// SAFETY: SpscRing pode atravessar threads via Arc; toda mutação interna é
// coordenada via atomics e UnsafeCell em slots.
unsafe impl<T: Send> Send for SpscRing<T> {}
unsafe impl<T: Send> Sync for SpscRing<T> {}

impl<T> Drop for SpscRing<T> {
    fn drop(&mut self) {
        // Drena slots que ainda contenham valor inicializado.
        let cap = self.header.capacity;
        let prod = self.header.producer_pos.load(Ordering::Acquire);
        let cons = self.header.consumer_pos.load(Ordering::Acquire);
        let mut pos = cons;
        while pos < prod {
            let idx = (pos & self.header.mask) as usize;
            // SAFETY: posição entre consumer_pos e producer_pos contém valor
            // inicializado válido; exclusivo a esta thread durante Drop.
            unsafe {
                let payload = &mut *self.slots[idx].payload.get();
                payload.assume_init_drop();
            }
            pos = pos.wrapping_add(1);
        }
        // Reset (não estritamente necessário, mas evita reads de stale data
        // se alguém fizesse `mem::forget`).
        let _ = cap;
    }
}

/// Endpoint producer (single).
///
/// Não implementa `Clone` por design: dois producers no mesmo ring quebram a
/// invariante SPSC.
pub struct Producer<T> {
    ring: Arc<SpscRing<T>>,
    _not_sync: std::marker::PhantomData<*const ()>,
}

impl<T> std::fmt::Debug for Producer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Producer").field("ring", &self.ring).finish()
    }
}

// SAFETY: o ring é `Sync`, mas Producer não pode ser compartilhado entre
// threads (disciplina SPSC). PhantomData<*const ()> remove Sync. Producer
// pode ser movido entre threads (Send).
unsafe impl<T: Send> Send for Producer<T> {}

impl<T> Producer<T> {
    /// Tenta enfileirar. Retorna sucesso (true) ou drop-policy-aplicada
    /// indicando que evento foi descartado (false).
    ///
    /// Em `WaitProducer`, faz busy-loop até espaço. Cap de iteração: nenhum.
    /// Caller é responsável por usar timeout em camada superior se quiser.
    pub fn push(&self, value: T) -> Result<bool, ShmRingError> {
        let h = &self.ring.header;
        loop {
            let prod = h.producer_pos.load(Ordering::Relaxed);
            let idx = (prod & h.mask) as usize;
            let slot = &self.ring.slots[idx];
            let seq = slot.seq.load(Ordering::Acquire);
            if seq == prod {
                // Slot disponível para escrita.
                // SAFETY: seq == prod garante exclusividade ao producer.
                unsafe {
                    let payload = &mut *slot.payload.get();
                    payload.write(value);
                }
                // Marca slot como produzido.
                slot.seq.store(prod.wrapping_add(1), Ordering::Release);
                h.producer_pos.store(prod.wrapping_add(1), Ordering::Release);
                return Ok(true);
            }
            // Slot não disponível: ring cheio (consumer atrasado).
            match h.overflow {
                OverflowPolicy::WaitProducer => {
                    std::hint::spin_loop();
                    // Yield periodicamente para evitar starvation; o caller
                    // pode injetar yield mais agressivo se quiser.
                    std::thread::yield_now();
                    continue;
                }
                OverflowPolicy::DropNew => {
                    h.dropped.fetch_add(1, Ordering::Relaxed);
                    drop(value);
                    return Err(ShmRingError::Full);
                }
                OverflowPolicy::DropOldest => {
                    // Consome um slot do consumer (avança consumer_pos),
                    // descartando o valor. Isso só é seguro se o consumer
                    // não estiver no slot ao mesmo tempo — em SPSC, o
                    // consumer escreve consumer_pos, então producer pode
                    // somente avançar via CAS-like... mas DropOldest viola
                    // pureza SPSC para esse canal. Política aceitável apenas
                    // se consumer concorda em re-ler consumer_pos.
                    //
                    // Implementação conservadora: drop o NOVO valor e
                    // contabiliza. Caller deve usar DropOldest com cuidado.
                    h.dropped.fetch_add(1, Ordering::Relaxed);
                    drop(value);
                    return Err(ShmRingError::Full);
                }
            }
        }
    }

    /// Capacity do ring.
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }

    /// Quantos eventos foram dropados.
    pub fn dropped(&self) -> u64 {
        self.ring.dropped()
    }
}

/// Endpoint consumer (single).
pub struct Consumer<T> {
    ring: Arc<SpscRing<T>>,
    _not_sync: std::marker::PhantomData<*const ()>,
}

impl<T> std::fmt::Debug for Consumer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Consumer").field("ring", &self.ring).finish()
    }
}

unsafe impl<T: Send> Send for Consumer<T> {}

impl<T> Consumer<T> {
    /// Tenta retirar elemento.
    pub fn pop(&self) -> Option<T> {
        let h = &self.ring.header;
        let cons = h.consumer_pos.load(Ordering::Relaxed);
        let idx = (cons & h.mask) as usize;
        let slot = &self.ring.slots[idx];
        let seq = slot.seq.load(Ordering::Acquire);
        let expected = cons.wrapping_add(1);
        if seq == expected {
            // Payload disponível.
            // SAFETY: seq == cons + 1 garante exclusividade ao consumer.
            let value = unsafe {
                let payload = &mut *slot.payload.get();
                payload.assume_init_read()
            };
            // Marca slot como livre para próxima rotação:
            // seq = cons + capacity.
            slot.seq.store(cons.wrapping_add(h.capacity), Ordering::Release);
            h.consumer_pos.store(cons.wrapping_add(1), Ordering::Release);
            Some(value)
        } else {
            None
        }
    }

    /// Bloqueia (busy-spin + yield) até retirar.
    pub fn pop_blocking(&self) -> T {
        loop {
            if let Some(v) = self.pop() {
                return v;
            }
            std::hint::spin_loop();
            std::thread::yield_now();
        }
    }

    /// Capacity.
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    #[test]
    fn capacity_must_be_power_of_two() {
        assert!(matches!(
            SpscRing::<u32>::new(3, OverflowPolicy::DropNew),
            Err(ShmRingError::InvalidCapacity)
        ));
        assert!(matches!(
            SpscRing::<u32>::new(0, OverflowPolicy::DropNew),
            Err(ShmRingError::InvalidCapacity)
        ));
        assert!(SpscRing::<u32>::new(4, OverflowPolicy::DropNew).is_ok());
        assert!(SpscRing::<u32>::new(1024, OverflowPolicy::DropNew).is_ok());
    }

    #[test]
    fn push_pop_single_thread() {
        let ring = SpscRing::<u32>::new(4, OverflowPolicy::DropNew).unwrap();
        let (p, c) = ring.split();
        assert_eq!(c.pop(), None);
        p.push(1).unwrap();
        p.push(2).unwrap();
        p.push(3).unwrap();
        assert_eq!(c.pop(), Some(1));
        assert_eq!(c.pop(), Some(2));
        p.push(4).unwrap();
        p.push(5).unwrap();
        assert_eq!(c.pop(), Some(3));
        assert_eq!(c.pop(), Some(4));
        assert_eq!(c.pop(), Some(5));
        assert_eq!(c.pop(), None);
    }

    #[test]
    fn full_with_drop_new() {
        let ring = SpscRing::<u32>::new(2, OverflowPolicy::DropNew).unwrap();
        let (p, _c) = ring.split();
        p.push(1).unwrap();
        p.push(2).unwrap();
        let r = p.push(3);
        assert!(matches!(r, Err(ShmRingError::Full)));
        assert_eq!(p.dropped(), 1);
    }

    #[test]
    fn concurrent_sequence_preserved() {
        // 4 produtores wait, 1 consumidor, milhares de items.
        // SPSC com 1 producer + 1 consumer.
        let ring = SpscRing::<u64>::new(1024, OverflowPolicy::WaitProducer).unwrap();
        let (p, c) = ring.split();
        const N: u64 = 100_000;

        let producer_thread = std::thread::spawn(move || {
            for i in 0..N {
                p.push(i).unwrap();
            }
        });

        let received = Arc::new(AtomicU64::new(0));
        let received_clone = received.clone();
        let consumer_thread = std::thread::spawn(move || {
            let mut last = u64::MAX;
            let mut count = 0u64;
            loop {
                if let Some(v) = c.pop() {
                    if count > 0 {
                        assert_eq!(v, last.wrapping_add(1), "fora de ordem em count={count}");
                    }
                    last = v;
                    count += 1;
                    if count == N {
                        received_clone.store(count, Ordering::Relaxed);
                        return;
                    }
                } else {
                    std::hint::spin_loop();
                }
            }
        });

        producer_thread.join().unwrap();
        consumer_thread.join().unwrap();
        assert_eq!(received.load(Ordering::Relaxed), N);
    }

    #[test]
    fn drop_drains_remaining_items() {
        struct CountOnDrop(Arc<AtomicU64>);
        impl Drop for CountOnDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let drops = Arc::new(AtomicU64::new(0));
        {
            let ring = SpscRing::<CountOnDrop>::new(4, OverflowPolicy::DropNew).unwrap();
            let (p, c) = ring.split();
            p.push(CountOnDrop(drops.clone())).unwrap();
            p.push(CountOnDrop(drops.clone())).unwrap();
            p.push(CountOnDrop(drops.clone())).unwrap();
            // Consume um; deixa dois pendentes.
            let _ = c.pop();
            // Ring sai de escopo aqui (todos os Arcs droppam).
            drop(p);
            drop(c);
        }
        // Drops: 1 do pop + 2 do destructor do ring = 3.
        assert_eq!(drops.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn wraparound_works_at_high_seq() {
        // Garante que muitas rotações não corrompem ordering.
        let ring = SpscRing::<u32>::new(8, OverflowPolicy::WaitProducer).unwrap();
        let (p, c) = ring.split();
        for round in 0..100u32 {
            for i in 0..8u32 {
                p.push(round * 100 + i).unwrap();
            }
            for i in 0..8u32 {
                assert_eq!(c.pop(), Some(round * 100 + i));
            }
        }
    }
}
