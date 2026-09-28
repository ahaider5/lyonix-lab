# LYONIX-LAB Roadmap

No dates or estimates are attached to any phase. Status reflects the repository as of Core Architecture Baseline v1.

## Phase A/B — Architecture and research

- **Status:** Complete (pre-implementation).
- **Objective:** Approve the architecture: headless Rust core, model/artifact separation, feasibility-first recommendation, runtime adapter contract, managed process lifecycle.
- **Implemented items:** Documented in `docs/ADR-001-headless-rust-core.md` and `docs/ADR-002-tauri-ipc-ui.md`.
- **Remaining work:** None.
- **Exit criteria:** Approved architecture document; satisfied.

## Phase C — Headless core: domain, catalog, hardware, recommendation

- **Status:** Complete.
- **Objective:** Runtime-neutral domain types, catalog ingestion from user-configured model roots, hardware detection, feasibility-first recommendation.
- **Implemented items:**
  - Typed domain models (hardware, models, artifacts, installed models, profiles, runtime, inference, metrics).
  - `ModelDefinition` / `ModelArtifact` / `InstalledModel` separation with `CatalogEntry` coupling.
  - `models.json` (legacy schema) ingestion; Qwen3.5 filename mismatch corrected in the catalog.
  - Configurable model roots via user configuration; `installed_models(roots)` with path-safety enforcement (absolute/traversal/prefix rejection, root confinement).
  - CPU/RAM discovery via `sysinfo`; `llama-server --list-devices` parsing; platform-aware native GPU probes.
  - Hardware tier as derived metadata.
  - Feasibility checks before scoring; explainable recommendations with alternatives/rejections.
  - Semantic `RuntimeConfiguration` independent of llama.cpp CLI terminology.
- **Remaining work:** None for this baseline.
- **Exit criteria:** Catalog/discovery tests pass against the real configured model root; satisfied.

## Phase D — Runtime lifecycle: adapter, supervisor, health, durable state

- **Status:** Complete.
- **Objective:** Prove the real llama.cpp lifecycle end-to-end without a UI.
- **Implemented items:**
  - llama.cpp runtime adapter (`RuntimeConfiguration` → `LaunchPlan` translation).
  - `ProcessSupervisor` with owned-child management, PID/fingerprint identity, and per-runtime stdout/stderr logs.
  - Runtime health polling with lifecycle states (stopped/loading/ready/failure).
  - Durable runtime state (`runtime-state.json`) with save/load/clear.
  - Localhost-only binding; managed host/port/model/context arguments; restricted `customArgs` escape hatch.
  - Real-machine lifecycle validation on the development machine (Windows, i5-10210U, MX250): configured model root → discovery → resolution → LaunchPlan → supervisor start → `/health` READY → clean stop.
- **Remaining work:** None for this baseline.
- **Exit criteria:** Real-machine smoke test proved start/READY/stop with owned PID tracking and clean port closure; satisfied.

## Phase E — Inference layer

- **Status:** Complete.
- **Objective:** Smallest production-quality inference layer for the running local model.
- **Implemented items:**
  - Typed inference API: `ChatRequest`, `ChatResponse`, `ChatStreamEvent` (Delta/Done), `InferenceError` (NotReady, InvalidRequest, NonLocalhost, Http, Io, Status).
  - `LlamaCppInferenceClient` over the managed localhost endpoint (`http://127.0.0.1:8081/v1`).
  - Blocking and streaming inference; SSE parsing behind the runtime boundary.
  - Unit tests for serialization; mocked HTTP tests for success and error responses.
  - Real-machine blocking and streaming validation against the Spark model (multi-delta stream, final Done, clean stop, port closed).
- **Remaining work:** None for this baseline.
- **Exit criteria:** 25 tests pass; real streaming smoke test passes; satisfied.

## Phase F — Application-service boundary (CLI readiness)

- **Status:** Complete.
- **Objective:** A facade so the CLI does not orchestrate catalog/recommendation/runtime/inference logic itself; durable cross-invocation runtime state; one-process `run` workflow.
- **Implemented items:**
  - `LyonixLab` facade with typed workflows: `doctor`, `list_models`, `recommend`, `plan`, `start`, `wait_ready`, `status`, `chat`, `chat_stream`, `stop`, `run`.
  - `ProcessSupervisor::stop_by_pid` — PID + executable identity validation for short-lived invocations.
  - Facade/status/chat unit tests; real-machine streaming smoke test through the facade.
- **Remaining work:** None for this baseline.
- **Exit criteria:** Full validation gate (`cargo check`, `cargo test`, `cargo check --all-targets`, `clippy -D warnings`) passes; streaming smoke test passes; satisfied.

## Phase G — CLI implementation

- **Status:** Complete.
- **Objective:** A real CLI binary over `LyonixLab` workflows (doctor/list/recommend/start/status/chat/stop/run), using the persisted runtime state for cross-invocation reconciliation.
- **Implemented items:**
  - `lyonix` CLI crate (`cli/`): thin binary over the facade; hand-rolled argument parsing (no external CLI-framework dependency); exit codes 0 success / 1 operational error / 2 usage error; repository-root discovery via nearest ancestor containing `models.json`.
  - All eight commands: `doctor` (JSON report), `models` (pretty-JSON list), `recommend [profile]` (case-insensitive profile selector, `TaskProfile` as a request parameter only), `start [profile]` (facade recommend → start, 240 s READY timeout), `status` (reconciled from durable state), `stop` (facade-only, `ProcessSupervisor::stop_by_pid` identity-verified kill), `chat <prompt...>` (blocking, plain-text output, request validated first), `run <prompt...>` (facade one-process lifecycle, cleanup-safe).
  - 19 arg-parsing tests; no test calls `LyonixLab::open` or launches `llama-server`.
  - Real-machine validation on the development machine (Windows, i5-10210U, MX250): full cross-invocation lifecycle (start → status Ready from a separate invocation → stop from a separate invocation, process gone, state cleared); stale-record reconciliation; live `chat` and live `run` with real model responses; `PortInUse` fail-safe; per-profile recommendations (Spark for chat, Qwen2.5 Coder for coding).
- **Remaining work:** Optional follow-ups only (`--stream`, per-invocation sampling overrides, table formatting, `Failed`/`Crashed` reconciliation state). No business logic in the CLI layer.
- **Exit criteria:** CLI exercises every `LyonixLab` workflow; cross-invocation start/status/stop works via durable state; validation gate passes. Satisfied.

## Phase H — Tauri desktop UI as an application-service consumer

- **Status:** Partially implemented (exists from an earlier milestone; not the current core milestone).
- **Objective:** Desktop UI as a thin IPC client over the same application services.
- **Implemented items:** Tauri 2 shell, Svelte 5 + TypeScript + Vite frontend, typed IPC commands, dashboard/models/runtime/profiles/settings/diagnostics views, minimal capability set, restricted CSP (structurally checked in the earlier milestone's environment).
- **Remaining work:** Validate on real machines; migrate the IPC commands onto the `LyonixLab` facade; live log streaming; production packaging/signing/updater.
- **Exit criteria:** UI runs against the validated core on a supported machine without business logic in components; validation gate passes.

## Explicitly deferred (no phase assigned)

- Ollama / LM Studio / MLX / vLLM runtime adapters.
- Model download/install service.
- Managed runtime distribution/sidecars.
- SQLite persistence.
- Benchmark execution/calibration engine.
- Persistent process recovery across application restarts.
- Dynamic plugins.
