# Failure Modes

Catálogo de modos de falha esperados e tratamento canônico. Cada modo tem
detector, severidade, ação automática, e ação humana.

## Categoria 1: Conectividade venue

### F-1.1 — WS disconnect curto (<5s)
- **Detector:** ping/pong timeout ou socket close.
- **Severidade:** baixa.
- **Auto:** reconnect imediato com backoff exponencial (50ms, 100ms, 200ms, …).
- **UI:** indicador da venue muda para amarelo brevemente.
- **Quality:** confidence rebaixa em ~5pts; restaura ao reconnect.

### F-1.2 — WS disconnect longo (>30s)
- **Auto:** reconnect; ao reconectar, REST snapshot do orderbook + diff de
  user-data stream desde última sequence conhecida.
- **Risk Daemon:** entra em `VenueDegraded`; bloqueia novos intents para essa
  venue até reconciliation.
- **UI:** banner "VENUE X RECONNECTING".

### F-1.3 — Sequence gap em depth stream
- **Detector:** `update_id` ou `sequence_id` skip.
- **Auto:** book passa a `GapDetected`; REST snapshot solicitado; deltas
  bufferizados; ao chegar snapshot, descartar deltas anteriores e aplicar
  buffer.
- **UI:** chart marca período com confidence baixo; indicadores derivados
  ficam degraded.

### F-1.4 — Snapshot REST timeout/erro
- **Auto:** retry com backoff até 3 tentativas; depois marca canal como
  `Resyncing` indefinidamente (mas ainda recebendo deltas) com banner.
- **Risk Daemon:** `VenueDegraded` para o símbolo afetado.

### F-1.5 — Rate limit response (429)
- **Detector:** HTTP 429 ou WS rate-limit mensagem.
- **Auto:** respeita `Retry-After`; reduz subscription rate; loga.
- **Risk Daemon:** se for em path de execução, marca venue degraded e
  considera fallback para outra venue se smart-routing está ativo.

### F-1.6 — Venue maintenance/halt
- **Detector:** payload da venue ou status endpoint.
- **Auto:** `VenueStatus::Maintenance`; UI mostra; novos intents rejeitados
  para essa venue.

## Categoria 2: Data plane interno

### F-2.1 — Decoder/normalizer panic
- **Detector:** panic hook por venue task.
- **Auto:** task isolada reinicia; ingestor mantém estado conhecido; canal
  marcado como `Degraded` por janela de N segundos.
- **Audit:** event `DecoderRecovered`.

### F-2.2 — SHM ring full
- **Detector:** producer detecta `write_seq - read_seq >= capacity`.
- **Política:**
  - Canais `NEVER_DROP` (raw events, book deltas): producer faz backpressure
    (slow down decoder) até space disponível; se overflow inevitável, marca
    canal como `DataGap` e dispara snapshot.
  - Canais `COALESCE`: combina updates em janela de 16ms.
  - Canais `DEGRADE`: skip event + counter incrementa.
- **UI:** banner "INDICATORS THROTTLED" se canal DEGRADE excedeu threshold.

### F-2.3 — Storage write atrasado
- **Detector:** writer queue depth > threshold por > N segundos.
- **Auto:** ring de overflow; quando volta ao normal, drena. Eventos não
  perdem mas ficam "not yet durable" — marca period como tal em segment-index.
- **UI:** indicator de storage health.

### F-2.4 — Disk full
- **Detector:** OS error no write.
- **Auto:** Data Plane entra em modo "memory-only"; alertas; pause de
  ingestão de canais não-críticos.
- **Humano:** liberar espaço.

### F-2.5 — Orderbook invariant violation
- **Detector:** `cfg(debug_assertions)` audit após cada apply, periódico em
  release.
- **Auto:** book marcado como `Invalid`; força resync via snapshot REST.
- **Audit:** dump de últimos N events para análise.

## Categoria 3: Risk Daemon

### F-3.1 — Intent exceeds envelope
- **Detector:** validação determinística.
- **Resultado:** `Rejected{reason}` retornado para sender; intent **não vira
  ordem**. Contado em métricas.
- **UI:** notificação ao trader.

### F-3.2 — Order submit timeout
- **Auto:** query order by `client_order_id`; se found, adopt; se not found,
  retry com **mesmo** id; se ambíguo após N tentativas, bloquear novos intents
  para o símbolo até reconciliation manual.
- **Audit:** todos os passos.

### F-3.3 — Reconciliation diff
- **Detector:** periodic snapshot comparison ou pós-restart.
- **Auto:**
  - Venue tem ordem que local não tem → adopt + log.
  - Local tem ordem que venue não tem → marcar `Lost` + alerta + investigar.
  - Estado diverge → adopt venue como autoridade + log.
- **Trigger:** se diff > threshold ou inclui posição não esperada → `PanicSafe`.

### F-3.4 — Killswitch fired
- **Auto:** cancel all orders; flatten opcional (config); block new intents;
  notifications redundantes.
- **Audit:** snapshot completo do estado.

### F-3.5 — Heartbeat from Terminal lost (modo AUTOMATION)
- **Auto:** política configurable: pause strategy (default), keep going,
  flatten.
- **Humano:** check Terminal.

### F-3.6 — Venue retorna duplicate client_order_id
- **Detector:** explicit error code da venue.
- **Auto:** query order; verificar estado; se já preenchida ou working, adopt;
  audit log com correlação.

### F-3.7 — User data stream out-of-order
- **Detector:** sequence/id retrocede.
- **Auto:** buffer + reorder janela curta; se persiste, REST poll mais
  frequente até resolver.

## Categoria 4: Terminal

### F-4.1 — Render thread panic
- **Auto:** janela isolada; outras janelas continuam; ao recriar janela,
  workspace state é restaurado.

### F-4.2 — WebView crash (chrome)
- **Auto:** chrome restart; janelas wgpu nativas continuam (não compartilham
  processo do WebView).

### F-4.3 — Frame budget exceeded persistente
- **Detector:** P99 frame time > 16ms por > 5s.
- **Auto:** auto-degrade (LOD para footprint, decimation agressiva, suspend
  drawing detection auto).
- **UI:** indicator "PERFORMANCE DEGRADED".

### F-4.4 — Hotkey accidental em modo errado
- **Mitigação:** mode-lock; cooldown entre modos destrutivos; foco-aware (só
  dispara se Terminal é janela ativa); confirmação timer-based para reverse.

## Categoria 5: Sistema

### F-5.1 — Clock jump (NTP, sleep/wake)
- **Detector:** monotonic time vs wall time check.
- **Auto:** marcar período como `ClockDegraded`; eventos derivam timestamps
  de exchange_ts preferentially; correlations cross-venue suspended até
  estabilizar.

### F-5.2 — Process kill (OOM, signal)
- **Auto (cada processo):** crash handler dump sanitized; supervisor
  re-spawn; reconciliation obrigatória ao subir.

### F-5.3 — Permission denied (keychain)
- **Detector:** Risk Daemon ao tentar carregar credenciais.
- **Auto:** ficar em `NoCredentials`; Terminal mostra prompt para usuário
  reautenticar.
- **Humano:** desbloquear keychain.

## Estados terminais

Quando algo é irrecuperável automaticamente, o componente entra em estado
seguro e exige humano:

- Risk Daemon: `PanicSafe` — cancela o que pode, abre alerta resiliente,
  recusa intents.
- Data Plane: `ReadOnly` — não tenta novos subscribes; serve dados gravados;
  reconnect manual.
- Terminal: prompt full-screen com diagnostic + opção de "view degraded" ou
  restart.

## Princípio

> Falhar visivelmente é melhor do que falhar silenciosamente. Falhar de modo
> seguro é melhor do que falhar de modo recuperável que ninguém percebe.
