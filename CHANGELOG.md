# Changelog

All notable changes to LYONIX-LAB are documented here. The project has no prior releases; the entry below describes the functionality actually present at Core Architecture Baseline v1. No release history is fabricated.

## Unreleased — Core Architecture Baseline v1

### Added

- Headless Rust core (`lyonix-core`) with layered architecture: domain, catalog, hardware, recommendation, application, runtime, config, persistence, diagnostics.
- Typed domain models for hardware, models, artifacts, installed models, profiles, runtime state/configuration, recommendations, inference, and metrics.
- `ModelDefinition` / `ModelArtifact` / `InstalledModel` separation with `CatalogEntry` coupling; `models.json` (legacy schema) ingestion without absolute user paths in catalog data.
- Configurable model roots via user configuration (`config.toml` `[[model_roots]]`); installed-model discovery with path-safety enforcement (absolute/traversal/prefix rejection, root confinement); corrected the Qwen3.5 catalog filename to match the real artifact.
- Hardware detection: CPU/RAM via `sysinfo`, `llama-server --list-devices` parsing with order-preserving device discovery, platform-aware native GPU probes (`nvidia-smi`, `system_profiler`) with runtime-device fallback, derived hardware tiers.
- Feasibility-first recommendation pipeline: capability/compatibility/resource gates before scoring, explainable output with alternatives and rejections, semantic `RuntimeConfiguration` independent of llama.cpp CLI terminology, catalog `default_profile` fallback for model selection.
- llama.cpp runtime adapter: `RuntimeConfiguration` → `LaunchPlan` translation, managed host/port/model/context arguments, `--fit`/automatic GPU-layer policy, restricted `customArgs` escape hatch (cannot override managed settings or enable `--tools`), built-in host tools disabled.
- `ProcessSupervisor`: owned-child process management with PID/fingerprint identity, per-runtime stdout/stderr logs, `stop` (child-handle ownership check) and `stop_by_pid` (PID + executable identity validation for short-lived invocations). Name-based process killing is prohibited.
- Runtime health polling with lifecycle states (stopped/loading/ready/failure); durable runtime state persisted to `{state_dir}/runtime-state.json` (pid, executable, fingerprint, endpoint, model/artifact id, start time, log paths) with save/load/clear.
- Typed inference layer: `LlamaCppInferenceClient` over the managed localhost endpoint (`http://127.0.0.1:8081/v1`), blocking `chat()`, streaming `chat_stream()` with SSE parsing into `Delta`/`Done` events, `InferenceError` variants (NotReady, InvalidRequest, NonLocalhost, Http, Io, Status), `single_turn_request` convenience.
- `LyonixLab` application facade: `doctor`, `list_models`, `recommend`, `plan`, `start`, `wait_ready`, `status`, `chat`, `chat_stream`, `stop`, and a cleanup-safe `run` workflow owning the full lifecycle (recommend → start → READY → inference → stop) in one process.
- `LyonixConfigDir` environment-variable override for config/state directory isolation (tests).
- Test suite: 25 tests covering device ordering, feasibility, reasoning defaults, resolver selection, tool safety, network policy, catalog discovery against the real configured model root, path-safety rejection, persistence round-trips, facade workflows, and mocked HTTP success/error responses.
- Real-machine smoke tests (gated behind `LYONIX_RUNTIME_SMOKE=1`, safe by default): lifecycle smoke (resolve → start → READY → deterministic prompt → non-empty response → stop → port closed) and application-facade streaming smoke (recommend → start → status reconciles Ready → multiple Delta events → final Done → stop → port closed). Both validated on the development machine (Windows, Intel i5-10210U, GeForce MX250) against the Spark X2.5 1.7B model.
- Documentation set: `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/DEVELOPMENT.md`, `docs/SECURITY.md`, `docs/RUNTIME-ADAPTERS.md`, `CONTRIBUTING.md`, plus existing `docs/ADR-001` / `docs/ADR-002`.

### Validation

- `cargo check`, `cargo test` (25/25), `cargo check --all-targets`, and `cargo clippy --all-targets --all-features -- -D warnings` (zero warnings) pass as of this baseline.

### Not in this baseline

- Tauri UI as a consumer of the application facade (shell exists from an earlier milestone; not the current core milestone).
- Ollama / LM Studio / MLX / vLLM runtime adapters, model downloads, managed runtime distribution, SQLite persistence, benchmark execution, production packaging/signing/updater, live log streaming.

## Unreleased — Phase G: CLI

### Added

- `lyonix` CLI crate (`cli/`): a thin binary over the `LyonixLab` application facade. Hand-rolled argument parsing (no external CLI-framework dependency); exit codes: 0 success, 1 operational error, 2 usage error. Repository root discovery walks up from the working directory to the nearest ancestor containing `models.json`.
- `lyonix doctor` — JSON `DiagnosticReport` of hardware, runtimes, model roots, and installed-artifact count via `LyonixLab::doctor()`.
- `lyonix models` — pretty-JSON list of installed catalog models with local paths via `LyonixLab::list_models()`.
- `lyonix recommend [profile]` — facade recommendation with an optional task-profile selector (`chat`, `coding`, `reasoning`, `vision`, `agent`, `longcontext`; case-insensitive; `chat` default). The `TaskProfile` is constructed as a request parameter only; no scoring or thresholds live in the CLI.
- `lyonix start [profile]` — facade `recommend` → `start` (240 s READY timeout); prints pid/endpoint/model/artifact JSON. Cross-invocation lifecycle persists durable runtime state.
- `lyonix status` — reconciled `RuntimeStatus` from durable state plus a live health probe via `LyonixLab::status()`.
- `lyonix stop` — stops through the facade only (`ProcessSupervisor::stop_by_pid` with PID + executable identity verification); prints `{"stopped": true}`.
- `lyonix chat <prompt...>` — blocking chat completion via `LyonixLab::chat` (plain-text output, request validated first, 256-token CLI default, 120 s timeout).
- `lyonix run <prompt...>` — the facade's one-process lifecycle via `LyonixLab::run` (recommend → start → READY → inference → stop, cleanup-safe).
- CLI test suite: 19 arg-parsing tests. No test calls `LyonixLab::open` (reads real user configuration/hardware) or launches `llama-server`.

### Validation

- `cargo check`, `cargo test` (19/19 CLI tests; 25/25 core tests), `cargo check --all-targets`, and `cargo clippy --all-targets --all-features -- -D warnings` (zero warnings) pass for the CLI crate; `core/` untouched and green.
- Real-machine smoke (Windows, i5-10210U, MX250): `doctor` prints the real report; `models` lists the three catalog models with real local paths; per-profile `recommend` selects expected models (Spark for chat, Qwen2.5 Coder for coding); cross-invocation `start` → `status` Ready (separate invocation, durable state) → `stop` (separate invocation, identity-verified kill, process gone, state cleared); stale-record reconciliation verified; live `chat` and live `run` return real model responses; `run` cleans up (process gone, state cleared, port closed); `PortInUse` fail-safe verified while a runtime is up; invalid/extra arguments exit 2.

### Not in this phase

- Streaming chat output (`--stream`), per-invocation `--max-tokens`/sampling overrides, human-readable table formatting for `doctor`/`models` (functional JSON only by design).
- Tauri UI consumption of the application facade (Phase H).
- Additional runtimes, model downloads, SQLite, benchmark/calibration, packaging.
