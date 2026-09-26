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

- CLI binary (application boundary ready; parser not built).
- Tauri UI as a consumer of the application facade (shell exists from an earlier milestone; not the current core milestone).
- Ollama / LM Studio / MLX / vLLM runtime adapters, model downloads, managed runtime distribution, SQLite persistence, benchmark execution, production packaging/signing/updater, live log streaming.
