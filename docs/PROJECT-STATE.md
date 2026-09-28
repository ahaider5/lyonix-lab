# LYONIX-LAB Project State

This file records the current implementation state for agents.
Architecture decisions belong in `docs/ARCHITECTURE.md`.
The roadmap belongs in `docs/ROADMAP.md`.
GitHub Issues are the authoritative task queue.

## Project

**Name:** LYONIX-LAB  
**Repository:** `lyonix-lab`  
**Rust core:** `lyonix-core`  
**CLI:** `lyonix` (planned)

## Current Milestone

**Phase G — CLI: complete**

The first functional CLI (`lyonix`, crate `cli/`) over the existing
`LyonixLab` application-service facade is implemented, validated, and
merged. The CLI is a thin consumer of application workflows.

## Current Phase

**Phase G — CLI (complete)**

All eight commands are implemented and validated on the real machine:
`doctor`, `models`, `recommend`, `start`, `status`, `chat`, `stop`, `run`.

## Completed

### CLI (Phase G)
- `lyonix` CLI crate (`cli/`): thin binary over `LyonixLab`; hand-rolled
  argument parsing; exit codes 0/1/2 (success/operational/usage)
- Commands: `doctor`, `models`, `recommend [profile]`, `start [profile]`,
  `status`, `chat <prompt...>`, `stop`, `run <prompt...>`
- Cross-invocation start/status/stop via durable runtime state and
  `ProcessSupervisor::stop_by_pid` (identity-verified stops)
- 19 arg-parsing tests (no facade calls or llama-server launches in tests)
- Real-machine validation: full lifecycle, live chat/run, stale-record
  reconciliation, PortInUse fail-safe, per-profile recommendations

### Core
- Runtime-neutral Rust domain model
- ModelDefinition / ModelArtifact / InstalledModel separation
- Configurable model roots
- Model path-safety enforcement
- Hardware detection
- llama.cpp runtime discovery
- Feasibility-first recommendation pipeline
- Semantic RuntimeConfiguration
- llama.cpp runtime adapter
- ProcessSupervisor with PID/executable identity validation
- Runtime health states
- Durable runtime state
- Localhost-only inference
- Blocking chat inference
- Streaming inference
- LyonixLab application facade

### Validation
- `cargo check` passes
- `cargo check --all-targets` passes
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- Full test suite passes repeatedly
- Real-machine llama.cpp lifecycle smoke test passes
- Real streaming inference smoke test passes

## Current Next Work

Phase H — Tauri desktop UI as an application-service consumer
(migrate the Tauri IPC commands onto the `LyonixLab` facade;
validate on real machines). See `docs/ROADMAP.md`.

Optional CLI follow-ups (small, not required for Phase G exit):
- `--stream` flag on `chat`
- `--max-tokens` / sampling overrides per invocation
- Human-readable table formatting for `doctor` / `models`
- `Failed`/`Crashed` status reconciliation state in the core

## Architecture Constraints

- CLI calls `LyonixLab`.
- CLI does not contain recommendation/business logic.
- CLI does not construct llama.cpp command lines directly.
- CLI does not manage processes directly.
- Runtime-specific behavior stays in runtime adapters.
- Do not bypass ProcessSupervisor.
- Do not weaken feasibility-before-scoring.
- Do not expose non-localhost inference by default.
- Do not introduce another runtime unless the issue explicitly
  requires it.
- Do not redesign the architecture to solve a local implementation
  inconvenience.

## MVP Strategy

Prioritize:

1. Functional
2. Correct
3. Observable
4. Testable
5. Usable
6. Visual polish

The first UI should be functional and minimal.
UI beautification is explicitly later work.

## Parallel-Agent Rules

- Parallel agents must work in isolated Git worktrees/branches.
- Never allow two agents to edit the same working tree simultaneously.
- Every implementation agent must report:
  - files changed
  - tests run
  - validation results
  - remaining risks
- Merge only after the parent/orchestrator reviews the result.

## Human Approval Required

Stop and request human review before:
- changing documented architecture
- changing security boundaries
- exposing network services
- adding a new runtime family
- introducing a major dependency
- changing persistence architecture
- changing public CLI behavior in a breaking way
- broad refactoring unrelated to the active issue

## Known Limitations

- llama.cpp is currently the only implemented runtime.
- Windows is the validated platform.
- Model downloading/install management is not implemented.
- SQLite is deferred.
- Benchmark/calibration system is deferred.
- Tauri exists but is not yet the primary application-service consumer.