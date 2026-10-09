# Argus delivery roadmap



| Area | Current state | Next acceptance criterion |
|---|---|---|
| Contracts and fixture pipeline | Implemented and locally tested | Validate the contract against a read-only venue feed |
| L2 book and sequence handling | Implemented; reference/property tests | Capture/replay real feed gaps without divergence |
| JSONL / checkpoints / replay | Implemented | Exercise long-running recovery; evaluate a columnar backend |
| SPSC transport | In-process foundation | Add OS-backed shared memory and independent-process tests |
| Observability | Text metrics and JSON logs | Add HTTP/OTLP export and end-to-end propagation |
| Terminal / live connectors | Planned | Read-only terminal and one read-only connector |
| Risk execution / paper trading | Primitives only | Simulated venue and reconciliation before any live routing |

[Project overview](../README.md) · [Original 28-phase plan](roadmap.md). A checked phase in the original plan means a foundation milestone; it does not imply every production subsystem is complete.


## Release gate

A production-ready claim requires reproducible deployment, recovery tests, security boundaries and measured runtime behavior; source inspection alone is not sufficient.
