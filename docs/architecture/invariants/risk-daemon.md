# Risk Daemon — Invariantes não-negociáveis

Estas invariantes valem para qualquer versão do Risk Daemon. Quebra requer ADR
explícita e plano de rollout supervisionado.

## I-1 — Único componente com credenciais
Risk Daemon é o **único** processo que carrega API keys de venue. Terminal,
Data Plane, Research e plugins **nunca** veem credentials.

## I-2 — Intent é imutável
Uma intent recebida não é mutada. Correções viram nova intent. Isso garante
auditabilidade.

## I-3 — Intent só vira ordem após validation completa
Validations: schema, envelope, capability da source, cooldown, rate limit,
time-of-day window, símbolo whitelist, daily loss cap, max margin usage.
Qualquer falha → `Rejected`.

## I-4 — `client_order_id` determinístico e idempotente
Formato: `argus-{account_hash}-{venue}-{intent_id}-{leg_index}`. Retry de
submit usa o mesmo ID. Venue rejeita duplicates → query state → adopt.

## I-5 — Reconciliation antes de aceitar novos intents
Após restart ou reconnect prolongado, fetch open orders + positions + recent
fills da venue; comparar com state local; resolver diffs; só então aceitar
novos intents.

## I-6 — Server-side stops/TPs prioritários quando confiáveis
Sempre preferir stop server-side. Client-side stops live exigem alerta visível
e modo opt-in explícito.

## I-7 — Audit log append-only com hash chain
Toda decisão (intent, validation, order, ack, fill, cancel, reconciliation,
killswitch) é gravada com `prev_hash` chained. Adultarações detectáveis via
`argus-audit verify`.

## I-8 — Risk envelope versionado
Mudanças no envelope: audit log com diff completo + cooldown obrigatório para
aumentos de limite + two-step confirmation para mudanças destrutivas.

## I-9 — Killswitch sempre disponível
Kill switch responde em < 100ms via hotkey local OU push mobile OU API local
autenticada. Mesmo sob load alto. Não pode ser bloqueado por fila de intents
pendente.

## I-10 — `f64`/`f32` proibidos
Risk Daemon usa apenas `i128` (`PriceTicks`, `QtyLots`, `MoneyMinor`). CI
enforça via grep.

## I-11 — Sem rede arbitrária
Risk Daemon só conecta a venues whitelist + Terminal IPC + Data Plane IPC +
audit log local. Sem Twitter, Discord, news, RPC arbitrário.

## I-12 — Watchdog interno
Risk Daemon tem subprocess monitor que detecta deadlock/panic do main loop
em < 5s e faz controlled restart com reconciliation obrigatória.

## I-13 — Nunca silenciar erros de execução
Falha de submit após N retries → marca intent como `ExecutionFailed`, alerta,
**não tenta de novo automaticamente**. Humano decide.

## I-14 — State machine explícita
Estados: `Booting`, `WaitingForConfig`, `NoCredentials`, `ReconcileRequired`,
`Ready`, `DataDegraded`, `VenueDegraded`, `ExecutionBlocked`,
`KillSwitchActive`, `PanicSafe`, `ShuttingDown`. Transitions documentadas.
Booleanos soltos proibidos.

## I-15 — Strategy não pode pedir exceção a envelope
Capability técnica (read X) e risk limit (max Y) são separados. Risk Daemon
aplica `min(envelope_global, envelope_strategy, envelope_user, venue_limits)`
— sempre o mais restritivo.

## I-16 — Reduce-only enforcement
Em close intents quando posição existe, reduce-only é forçado. Cancel/Replace
nunca quebra reduce-only enforcement.

## I-17 — Numeric precision policy aplicada por venue
Toda ordem enviada passa por `VenuePrecisionPolicy` (tick alignment, lot
alignment, min notional). Violação → `RejectedByLocalValidation`, nunca chega
à venue.

## I-18 — Sensitive types não vão para logs
Tipos marcados `Sensitive<T>` não implementam `Debug`. Tentativa de log via
trait causa erro de compilação.

## I-19 — Autoexecute não bypassa modo
EXPRESS, AUTOMATION, REVIEW, EXECUTE, VIEW são modos com permissões
diferentes. Switch entre modos exige aprovação explícita + cooldown entre
destrutivos.

## I-20 — Crash do Terminal não derruba ordens
Heartbeat do Terminal usado apenas para política configurável em AUTOMATION.
Em outros modos, ordens server-side seguem ativas mesmo sem Terminal.
