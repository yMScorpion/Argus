# ADR-0006 — Schema format: serde como ponte; Cap'n Proto como destino

## Status

Accepted (transitional)

## Contexto

Spec v2 prescreve **Cap'n Proto** para IPC tipado entre processos:
- Schemas versionados.
- Forward/backward compat policy explícita.
- Zero-copy reads.

Cap'n Proto requer:
- Compilador `capnp` (binário externo) instalado em build.
- Crate Rust `capnp` + `capnpc` para gerar bindings.
- Disciplina de schema evolution (campos sempre opcionais, etc).

Adicionar essa toolchain no commit zero da Fase 1 atrasa o roadmap sem ganho:
nenhum cliente externo consome ainda; testes determinísticos iniciais não
precisam de zero-copy.

## Decisão

**Fase atual (0-4):** definir tipos canonical em Rust com `serde` derive
(`Serialize`/`Deserialize`). Documentar shapes em `.capnp` files que vivem em
`schemas/capnp/` como **especificação canônica e futuro source-of-truth para
geração**.

**Fase 7+:** introduzir build script com `capnpc`, gerar bindings, migrar IPC
hot-path para Cap'n Proto. Tipos serde permanecem para storage Parquet
(serde-arrow), audit log (serde_json), e rotas JSON/REST.

## Alternativas consideradas

- **Cap'n Proto desde já:** atrasa entrega da Fase 1 sem benefício imediato;
  schemas vão evoluir muito nas Fases 0-4 e cada mudança vira PR no schema repo.
- **Protobuf:** alocação por encode/decode; não zero-copy.
- **FlatBuffers:** zero-copy, similar a Cap'n Proto; ecossistema Rust menor.
- **Bincode/postcard só:** sem versionamento explícito; difícil garantir
  compat.

## Consequências

### Positivas
- Velocidade de iteração inicial alta.
- Tipos Rust idiomáticos.
- Mesmo tipo serve serialização para múltiplos formatos (json, parquet, capnp
  no futuro).

### Negativas
- Migração para Cap'n Proto é refactor real, não trivial.
- Risco de schema drift entre `.capnp` declarado e tipo Rust real. Mitigação:
  schema-check tool em CI valida que campos existem em ambos.

### Neutras
- Schemas Cap'n Proto ficam como documentação executável da intenção.

## Invariantes

- Toda mensagem que cruza fronteira de processo tem `schema_version: u16` no
  header.
- Toda mensagem tem trace_id + sender_process + monotonic_ts_ns.
- Campos novos são adicionados, **nunca renomeados ou removidos** sem
  versionamento explícito.

## Testes obrigatórios

- `tools/schema-check`: lê `.capnp` files e verifica que campos correspondem
  a tipos Rust em `argus-schema`.
- Round-trip test: serializa, deserializa, compara igualdade estrutural.
- Backward-compat fixture: versão N consegue ler payload da versão N-1.
- Forward-compat fixture: versão N-1 lê payload N e ignora campos novos
  graciosamente.

## Plano de migração

1. Fases 0-4: serde + tipos Rust + `.capnp` files declarativos.
2. Fase 5-6: schema-check em CI.
3. Fase 7: introduzir `capnpc` em `argus-schema/build.rs`. Gerar tipos Rust
   paralelos a partir de `.capnp`. Migrar IPC bus.
4. Fase 8+: SHM hot-path adota representação Cap'n Proto packed quando
   beneficia.
