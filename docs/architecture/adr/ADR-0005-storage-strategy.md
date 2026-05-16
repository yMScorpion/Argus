# ADR-0005 — Storage: Parquet + DuckDB local-first

## Status

Accepted

## Contexto

A v1 dizia "ClickHouse embedded" — termo enganoso (CH não tem modo embedded
real; existe `chDB` mas é experimental). Precisamos:

- Hot tier para queries dos últimos 30 minutos (tick-level).
- Warm tier para histórico de dias/meses (analítico).
- Cold tier para histórico longo (arquivo).
- Replay determinístico com seek rápido (P99 < 200ms para janela arbitrária do
  último mês).
- Zero servidor externo no install default (usuário desktop).

## Decisão

**Hot tier:** memória — ring buffer por símbolo + arena de eventos recentes.

**Warm tier:** **Parquet local + DuckDB para queries**.
- Parquet com particionamento `venue=X/symbol=Y/date=YYYY-MM-DD/`
- Compressão ZSTD nível 9 (alto ratio, aceitável CPU em writer assíncrono).
- Schema versionado no metadata Parquet.
- Indexação por `(symbol, ts)` via segment-index file separado (B-tree em
  arquivo) para seek rápido.

**Cold tier:** mesmos arquivos Parquet, movidos para storage barato (S3, B2,
disco externo) após N dias. Re-attach via mount.

**Opcional avançado:** chDB ou ClickHouse server externo para usuários power.
Não default.

## Alternativas consideradas

- **SQLite:** insuficiente para colunas/agregação. OK para metadados.
- **RocksDB para tudo:** key-value não serve queries analíticas.
- **TimescaleDB:** requer PostgreSQL server; não default desktop.
- **InfluxDB:** servidor; tag cardinality issues com símbolos cripto.

## Consequências

### Positivas
- Sem servidor; install limpo.
- Parquet é formato aberto: zero lock-in; user pode abrir em pandas/polars
  diretamente.
- DuckDB in-process: queries SQL rápidas sem serialização de rede.
- Particionamento permite queries de range eficientes.

### Negativas
- Writes são em batch; latência de "evento gravado em disco" é maior do que um
  KV store. Mitigação: hot tier em memória; replay tier checkpoint frequente.
- Compaction de pequenos arquivos é necessária periodicamente.

### Neutras
- DuckDB não é hot-path; só para queries analíticas/replay/backtest.

## Invariantes

- Eventos só são considerados "duráveis" após fsync do batch.
- Audit log usa caminho separado (sync write append-only); não compartilha
  storage tier.
- Storage writer **nunca** bloqueia normalizer/orderbook engine. Backpressure
  vira evento de qualidade, não pause de hot path.
- Schema do Parquet inclui: `schema_version`, `code_version`, `venue`,
  `instrument`, intervalo temporal.

## Testes obrigatórios

- Replay verify: Parquet round-trip preserva todos campos do canonical event,
  incluindo timestamps (recv_ts_ns, exchange_ts_ns, process_ts_ns).
- Backpressure: storage writer atrasado não trava pipeline; eventos vão para
  ring de overflow; quando volta a normal, drena.
- Migration: schema_version antigo lido com bridge; novos campos viram default;
  removidos viram tombstone.
- Compaction: arquivos pequenos compactados sem perder eventos.
