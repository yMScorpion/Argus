# Ownership por agente

| Agente              | Pode alterar                            | Não pode alterar sem ADR     |
| ------------------- | --------------------------------------- | ---------------------------- |
| Schema Agent        | `schemas/`, `argus-schema/`             | lógica de execução           |
| Data Agent          | Data Plane, connectors, orderbook       | Risk Daemon                  |
| Risk Agent          | Risk Daemon, risk engine, execution     | Terminal UI                  |
| Terminal Agent      | render, chrome, drawings                | credenciais/execution        |
| Storage Agent       | Parquet/DuckDB/replay                   | schemas sem migração         |
| Observability Agent | metrics/tracing/logs                    | lógica de negócio crítica    |
| Test Agent          | fixtures, property tests, chaos tests   | produção sem issue vinculada |
| Security Agent      | secrets, sandbox, update, signing       | trading behavior             |
| Docs Agent          | ADRs, runbooks, invariants              | código crítico sem review    |

Regra-mãe: **um agente não pode resolver erro editando outro domínio sem
registrar ADR ou issue de contrato.**

## Definition of Done global

Uma tarefa só é "pronta" se tiver:

1. Código.
2. Teste unitário.
3. Teste de contrato, quando tocar schema/IPC.
4. Fixture ou golden file, quando tocar market data/replay/execution.
5. Métrica, quando tocar hot path.
6. Log estruturado, quando tocar estado/falha.
7. Documentação curta no módulo.
8. ADR, se mudou decisão arquitetural.
9. Benchmark, se tocar performance.
10. Runbook, se introduzir novo modo de falha operacional.
