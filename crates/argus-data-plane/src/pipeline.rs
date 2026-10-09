use std::sync::atomic::{AtomicU64, Ordering};

use argus_connectors_core::{MarketSubscription, VenueConnector};
use argus_errors::Result;
use argus_orderbook::OrderBook;
use argus_schema::EventPayload;
use argus_time::{Clock, ProcessTs};

use crate::publisher::PublishedChannel;
use crate::quality::ChannelQuality;

/// Estatísticas acumuladas do pipeline.
///
/// Estas são as métricas top-level. Métricas finas por canal vivem em
/// [`ChannelQuality`].
#[derive(Debug, Default)]
pub struct PipelineStats {
    /// Eventos processados (qualquer tipo).
    pub events_total: AtomicU64,
    /// Eventos rejeitados por sequence gap no orderbook.
    pub gaps_total: AtomicU64,
    /// Eventos late (after watermark).
    pub late_total: AtomicU64,
    /// Trades.
    pub trades_total: AtomicU64,
    /// Book deltas aplicados com sucesso.
    pub book_deltas_total: AtomicU64,
    /// Snapshots aplicados.
    pub book_snapshots_total: AtomicU64,
    /// Eventos publicados no SHM (incrementa apenas se publisher conectado).
    pub published_total: AtomicU64,
}

/// Pipeline elemento que une connector → normalizer → orderbook → quality →
/// publish.
///
/// Para a Fase 3 trabalhamos com um connector síncrono (fixture); na Fase 8+
/// migramos para tokio runtime com tasks isoladas por venue.
#[derive(Debug)]
pub struct Pipeline<C: VenueConnector> {
    connector: C,
    book: OrderBook,
    stats: PipelineStats,
    quality: ChannelQuality,
    publisher: Option<PublishedChannel>,
}

impl<C: VenueConnector> Pipeline<C> {
    /// Constrói pipeline com connector + orderbook vazio. Sem publisher.
    pub fn new(connector: C, book: OrderBook) -> Self {
        Self {
            connector,
            book,
            stats: PipelineStats::default(),
            quality: ChannelQuality::new(),
            publisher: None,
        }
    }

    /// Conecta um canal de publicação SHM. Eventos canônicos vão para o
    /// consumer associado.
    pub fn with_publisher(mut self, publisher: PublishedChannel) -> Self {
        self.publisher = Some(publisher);
        self
    }

    /// Roda subscription até o iterator do connector se esgotar.
    ///
    /// Para fixture connector, isso significa "consumir toda a sessão".
    /// Para connector real, seria loop infinito até cancelamento.
    pub fn run<K: Clock>(&mut self, sub: MarketSubscription, clock: &K) -> Result<()> {
        let stream = self.connector.subscribe(sub)?;
        for mut event in stream {
            // Atualiza process_ts com nosso clock.
            event.process_ts = ProcessTs::new(clock.now_unix());

            // Observa qualidade ANTES de aplicar — gap detection vs último
            // visto ainda funciona; spec exige isso para emitir QualityFlag::SequenceGap
            // se necessário.
            self.quality.observe(&event);

            self.stats.events_total.fetch_add(1, Ordering::Relaxed);
            match &event.payload {
                EventPayload::BookSnapshot(snap) => {
                    self.book.apply_snapshot(snap)?;
                    self.stats.book_snapshots_total.fetch_add(1, Ordering::Relaxed);
                }
                EventPayload::BookDelta(d) => match self.book.apply_delta(d) {
                    Ok(_) => {
                        self.stats.book_deltas_total.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        self.stats.gaps_total.fetch_add(1, Ordering::Relaxed);
                    }
                },
                EventPayload::Trade(_) => {
                    self.stats.trades_total.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }

            // Publish para SHM se conectado. Política `WaitProducer` significa
            // que se o consumer estiver atrasado, este loop espera — é
            // intencional: raw events são NEVER DROP.
            if let Some(pubchan) = &self.publisher {
                if pubchan.publish(event).is_ok() {
                    self.stats.published_total.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        Ok(())
    }

    /// Acesso ao orderbook atual (read-only).
    pub fn orderbook(&self) -> &OrderBook {
        &self.book
    }

    /// Acesso às estatísticas top-level.
    pub fn stats(&self) -> &PipelineStats {
        &self.stats
    }

    /// Acesso ao tracker de qualidade do canal.
    pub fn quality(&self) -> &ChannelQuality {
        &self.quality
    }

    /// Acesso ao publisher (read-only, para inspecionar `dropped()`).
    pub fn publisher(&self) -> Option<&PublishedChannel> {
        self.publisher.as_ref()
    }
}
