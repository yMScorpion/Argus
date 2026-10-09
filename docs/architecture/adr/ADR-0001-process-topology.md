# ADR-0001 — Topologia em quatro processos

## Status

Accepted

## Contexto

A v1 do ARGUS colapsava charting, ingestão de market data, execução e ML em um
único processo Tauri. Isso violava a regra "execução não compartilha domínio de
falha com UI": qualquer panic, deadlock ou vazamento em qualquer caminho punha
em risco posições abertas. Vulnerabilidades de WebView (Tauri usa
Chromium/WebKit/WebView2) ficavam a uma stack frame de credenciais de venue.

Trader profissional não pode aceitar que crash do chart cancele stops, que UI
travada bloqueie kill switch, ou que update da UI exija redeploy do engine de
execução.

## Decisão

ARGUS é dividido em **quatro processos** com fronteiras de IPC tipadas:

1. **Terminal** — render + drawings + emite intents. Sem credenciais.
2. **Data Plane** — ingest + normalize + storage + replay. Sem credenciais.
3. **Risk Daemon** — único componente com credenciais; valida intents, executa,
   reconcilia, mantém kill switch.
4. **Research/ML/Marketplace** — fora do hot path; backtest, treinamento, plugins.

Cada processo é binário independente, instalável e atualizável separadamente.

## Alternativas consideradas

- **Monoprocesso (v1):** mais simples de empacotar; rejeitado por blast radius.
- **Threads dentro de um processo com isolation lógica:** rejeitado porque OS
  panic em qualquer thread ainda derruba o processo inteiro; e WebView crash
  não é incomum.
- **Microsserviços via REST/gRPC local:** rejeitado porque latência de loopback
  HTTP no hot path é proibitiva (ms quando precisamos µs).

## Consequências

### Positivas
- Crash do Terminal não cancela ordens.
- Vulnerabilidades de WebView ficam isoladas das credenciais.
- Cada processo iterado independentemente; updates desacoplados.
- Risk Daemon tem superfície mínima e auditável.
- Restart de um processo não exige restart dos outros.

### Negativas
- Complexidade operacional: gerenciar quatro daemons.
- IPC tipado precisa schemas versionados desde o dia um.
- Ergonomia de "rodar tudo localmente" requer process supervisor.
- Debugging distribuído.

### Neutras
- Process supervisor ARGUS (futuro) lida com lifecycle.
- Single-binary "umbrella" pode existir como ergonomia para usuários que rodam
  tudo local — internamente ele apenas spawna os quatro.

## Invariantes

- Terminal **nunca** importa `argus-execution` ou `argus-risk` como dependência.
- Terminal **nunca** abre socket para venue.
- Risk Daemon **nunca** importa Tauri, WebView, plugin runtime ou render.
- Data Plane **nunca** envia ordens.
- Research **nunca** está no caminho síncrono de uma decisão de execução.

## Testes obrigatórios

- `cargo deny`-style check que verifica grafo de dependências entre crates.
- CI rejeita PR que adicione dependência cruzando uma fronteira proibida.
- Integration test que mata Terminal mid-trade e verifica que ordens server-side
  permanecem ativas.
- Integration test que mata Data Plane e verifica que Risk Daemon entra em
  estado conservador (não emite novos intents) sem perder posições.
