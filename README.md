# LYONIX-LAB

A local-LLM manager for Windows-class machines: it detects your hardware, discovers the GGUF models you already have installed, recommends a feasibility-first model/runtime configuration, runs `llama-server` as a supervised localhost-only process, and serves typed chat inference (blocking and streaming) over a small, tested Rust core.

LYONIX-LAB is in active development. The headless Rust core is the validated architecture baseline; a CLI and a Tauri desktop UI are the next consumers of the same core.

## What makes it different

- **Feasibility before scoring.** A model is only ranked if it can actually run on your machine. Infeasible models are rejected with reasons, not silently mis-fitted.
- **Runtime-neutral core.** llama.cpp-specific concepts never leak into the domain model — the same core is designed to host other runtimes later.
- **Owned process lifecycle.** Every `llama-server` is spawned, health-checked, and stopped by a process supervisor with PID + executable identity validation. No orphaned servers, no name-based killing of processes it doesn't own.
- **Durable runtime state.** Short-lived processes (like a CLI) can reconcile status and stop a runtime started by a previous invocation via a persisted state record — while the supervisor remains the only kill authority.
- **Localhost-only by default.** The managed runtime binds to `127.0.0.1:8081`; the inference client refuses non-local endpoints. No arbitrary HTTP URLs are exposed to UI layers.
- **Policy-based hardware fit.** Context size, threads, batches, KV cache type, and GPU-layer policy adapt to the detected machine (`-ngl auto` + `--fit on` when a dedicated GPU exists) — starting policies, not benchmark claims.

## Current capabilities

**Implemented and validated** (25 core tests + 19 CLI tests pass; `cargo check`, `cargo check --all-targets`, `clippy -D warnings` clean; real-machine lifecycle, streaming, and CLI validation on Windows / i5-10210U / GeForce MX250):

- Hardware detection: CPU/RAM, GPU adapters and VRAM, llama.cpp device discovery (`--list-devices`), derived hardware tiers.
- Model catalog and discovery: three-model catalog (`models.json`), configurable model roots (user config, path-safety enforced), `ModelDefinition` / `ModelArtifact` / `InstalledModel` separation, explicit `ModelResolver`.
- Recommendation: feasibility-first with explainable alternatives/rejections and a semantic `RuntimeConfiguration` independent of llama.cpp CLI terminology.
- llama.cpp runtime adapter, `LaunchPlan`, `ProcessSupervisor` (owned processes, PID/fingerprint identity, per-runtime logs, identity-verified stops), runtime health (stopped/loading/ready/failure), durable runtime state.
- Inference: typed chat request/response, blocking and streaming (SSE → Delta/Done events), typed errors including non-localhost refusal.
- Application facade (`LyonixLab`): `doctor`, `list_models`, `recommend`, `start`, `status`, `chat`, `chat_stream`, `stop`, and a one-process `run` workflow — the boundary a CLI/Tauri must use.
- Real-machine smoke tests (gated, safe by default) covering the full lifecycle and streaming.

## Current limitations

- **Tauri desktop shell exists but is not the current core milestone** — it was checked structurally in an earlier milestone, has not been validated against the current core on this machine, and does not yet consume the `LyonixLab` facade.
- Single runtime (llama.cpp only). Ollama, LM Studio, MLX, vLLM are explicitly deferred.
- No model downloading or managed runtime distribution — you must already have the GGUF files.
- No SQLite — JSON/JSONL persistence at this stage.
- Artifact integrity verification (SHA-256) is implemented but unused: the catalog carries no hashes.
- Status reconciliation does not distinguish a crashed-but-recorded runtime from a cleanly stopped one.
- Windows is the validated platform; macOS/Linux compile but their GPU probes are unvalidated.
- The legacy PowerShell launcher (`launcher.ps1`) remains in the repository as a reference path only.
- Hardware tiers and feasibility thresholds are policy, not benchmark claims.

## Roadmap

Phase-by-phase status, objectives, and exit criteria live in `docs/ROADMAP.md` (no dates or estimates). Summary:

- **Phases C–F (complete):** headless core — domain/catalog/hardware/recommendation, runtime lifecycle (adapter, supervisor, health, durable state), typed inference, application-service boundary.
- **Phase G (complete):** CLI over the `LyonixLab` workflows — `lyonix doctor/models/recommend/start/status/chat/stop/run`, cross-invocation lifecycle via durable runtime state.
- **Phase H (partially implemented):** Tauri desktop UI as an application-service consumer.
- **Deferred:** additional runtimes, model downloads, SQLite, benchmarks, packaging/updater.

## Development status

The Rust core (`core/`) is the architecture baseline: layered, typed, clippy-clean, and validated on real hardware. The architecture, security model, runtime adapter contract, and phase status are documented in `docs/`:

- `docs/ARCHITECTURE.md` — full architecture with implemented/planned/deferred labels
- `docs/SECURITY.md` — localhost boundary, process ownership, PID + identity validation, path safety
- `docs/RUNTIME-ADAPTERS.md` — the adapter boundary and the llama.cpp implementation
- `docs/DEVELOPMENT.md` — setup, testing, smoke tests, configuration locations
- `docs/ROADMAP.md` — phase status and exit criteria
- `CONTRIBUTING.md` — project structure, where code belongs, PR expectations
- `CHANGELOG.md` — baseline entry (no fabricated release history)
- `docs/ADR-001`, `docs/ADR-002` — accepted architecture decisions

## Quick start for developers

```text
git clone <repository>
cd lyonix-lab/core
cargo check
cargo test
```

Point the core at your model library (user configuration, never hardcoded):

1. Create `%APPDATA%\LYONIX-LAB\config.toml`:

   ```toml
   version = 1
   model_roots = ['D:\path\to\your\model\library']
   ```

2. `models.json` records each model's path relative to that root.

Run the real-machine streaming smoke test (requires a recent `llama-server` on PATH; safe-by-default — normal `cargo test` never launches it):

```text
cd core
LYONIX_RUNTIME_SMOKE=1 cargo test --test core_tests runtime_service_streaming_smoke -- --nocapture --test-threads=1
```

Validation gate for any change:

```text
cd core
cargo check
cargo test
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

See `CONTRIBUTING.md` before opening a PR.
