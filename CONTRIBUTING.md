# Contributing to LYONIX-LAB

## Project structure

```text
core/                  Headless Rust core (lyonix-core) — the architecture baseline
  src/domain/          Runtime-neutral types: hardware, models, profiles, runtime, inference, metrics
  src/catalog/         models.json ingestion, ModelResolver, path safety
  src/hardware/        CPU/RAM/GPU discovery, llama.cpp device parsing, hardware tiers
  src/recommendation/  Feasibility-first recommendation pipeline
  src/application/     LyonixLab facade, configuration service, typed workflows
  src/runtime/         llama.cpp adapter, ProcessSupervisor, health, inference client
  src/config/          Path resolution (config/state directories)
  src/persistence/     User config, durable runtime state, JSONL metrics
  src/diagnostics.rs   DiagnosticReport export
  tests/               Integration/fixture tests (core_tests.rs)
docs/                  Architecture, roadmap, development, security, runtime adapters, ADRs
app/                   Tauri 2 + Svelte desktop shell (earlier milestone; not the current core milestone)
models.json            Model catalog (definition + artifact + relative paths)
launcher.ps1, ...      Legacy PowerShell launcher (reference path only)
```

## Where code belongs

- **Domain types** (`core/src/domain/`): runtime-neutral data models. Nothing llama.cpp-specific here.
- **Application workflows** (`core/src/application/service.rs`): all orchestration of catalog/recommendation/runtime/inference. New consumer-facing behavior goes here as a `LyonixLab` workflow.
- **Runtime specifics** (`core/src/runtime/`): llama.cpp endpoint/CLI translation, supervision, inference protocol — behind the runtime/API boundary.
- **Catalog/persistence** (`core/src/catalog/`, `core/src/persistence/`): ingestion, resolution, path safety, durable state.

## Rules against business logic in CLI/Tauri

- A CLI (planned) or the Tauri UI must call only `LyonixLab` workflows. Neither may orchestrate catalog, recommendation, runtime, or inference logic itself.
- Neither may construct raw HTTP URLs or `llama-server` CLI arguments.
- Neither may spawn processes: `ProcessSupervisor` is the only spawn/stop authority, and name-based process killing is prohibited (identity-verified stops only — see `docs/SECURITY.md`).
- Neither may read or write model catalog files, user configuration, or runtime state directly.

## How to run tests

```text
cd core
cargo test
```

25 tests pass as of Baseline v1. Normal runs never launch `llama-server`.

Validation gate for any change:

```text
cd core
cargo check
cargo test
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

Clippy must stay clean (zero warnings, no `#[allow]`). Do not run `cargo fmt` globally.

## How to run smoke tests

Both smoke tests are gated behind `LYONIX_RUNTIME_SMOKE=1` and use the real configured model root and port 8081. Run one at a time:

```text
cd core
LYONIX_RUNTIME_SMOKE=1 cargo test --test core_tests runtime_service_streaming_smoke -- --nocapture --test-threads=1
```

See `docs/DEVELOPMENT.md` for the full smoke-test list and prerequisites (llama-server, model roots).

## How future runtime adapters should be contributed

1. Read `docs/RUNTIME-ADAPTERS.md` — it lists exactly what an adapter must provide.
2. Implement the translation, health mapping, and inference protocol under `core/src/runtime/<runtime_id>/`.
3. Add `"runtime_id"` to the affected artifacts' `runtime_compatibility` in `models.json`.
4. Do not weaken feasibility checks, modify recommendation scoring, or bypass `ProcessSupervisor`.
5. Validate with the full gate plus a real-machine smoke test for the new runtime.

## Basic PR expectations

- Scope: one milestone/feature per PR; no drive-by refactors or formatting changes.
- Validation: all four gate commands pass; smoke tests run when lifecycle behavior changes.
- Layering: business logic in the application layer; domain stays runtime-neutral; runtime details behind the boundary.
- Tests: new behavior comes with tests; mocked HTTP tests for client changes; no test may launch `llama-server` without the `LYONIX_RUNTIME_SMOKE=1` gate.
- Documentation: behavior changes update `CHANGELOG.md`; architectural changes update `docs/ARCHITECTURE.md` (status labels: implemented / partially implemented / planned / deferred / known limitation).
- No machine-specific absolute paths or secrets in source, tests, or catalog data.
