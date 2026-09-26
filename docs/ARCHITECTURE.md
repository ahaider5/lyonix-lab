# LYONIX-LAB Core Architecture

**Baseline v1** — this document describes the architecture as it exists in the repository today.

Status labels used throughout:

- **Implemented** — working, exercised by tests and/or real-machine validation.
- **Partially implemented** — exists but incomplete or unused in part.
- **Planned** — designed for, not yet built.
- **Deferred** — intentionally out of scope for this baseline.
- **Known limitation** — implemented behavior with a documented caveat.

## 1. Project purpose

LYONIX-LAB is a local-LLM manager for Windows-class machines. It detects hardware, discovers locally installed GGUF models from user-configured model roots, recommends a feasibility-first model/runtime configuration, runs llama.cpp (`llama-server`) as a supervised localhost-only process, and serves typed chat inference (blocking and streaming) against that runtime.

It replaces an earlier PowerShell launcher (`launcher.ps1`, kept as a legacy reference) with a headless Rust core designed to be consumed first by a CLI and later by a Tauri desktop UI.

## 2. Design goals

- Correctness and testability before GUI: the core is headless and fully testable without a display.
- Feasibility before scoring: a model is only scored if it can actually run on the detected hardware.
- Runtime-neutral domain and application layers: llama.cpp-specific concepts never become the domain model.
- Owned process lifecycle: no orphaned `llama-server`; every process is supervised and stoppable.
- Localhost-only networking by default.
- Typed errors everywhere (`thiserror` enums; no stringly-typed failure paths in the core).
- Explainable recommendations: alternatives and rejections carry reasons.
- No machine-specific absolute paths or secrets in catalog data.

## 3. Non-goals

- Multi-runtime support today (Ollama, LM Studio, MLX, vLLM).
- Model downloading or installation services.
- SQLite persistence.
- Remote or network-exposed inference endpoints.
- A generic shell/command-execution API.
- Benchmark execution or calibration.
- Production packaging, signing, or auto-updates.

## 4. Architecture overview

A single Rust crate (`lyonix-core`, under `core/`) with module-per-layer separation:

```text
CLI (Planned)      Tauri UI (exists in app/, thin IPC client; not the current core milestone)
        \             /
    application  — LyonixLab facade, configuration service, typed workflows
        |
       domain  — runtime-neutral types (hardware, models, profiles, runtime, inference, metrics)
        |
  catalog | hardware | recommendation | runtime (adapter, supervisor, health, state, inference)
        |            persistence (user config, durable runtime state, JSONL metrics) | diagnostics
```

Dependencies point inward: `application` depends on `domain`, `catalog`, `hardware`, `runtime`, and `persistence`; `domain` depends on nothing internal. The Tauri app (`app/`) is a separate binary that is a thin client over the core (see ADR-002).

## 5. Domain model

All types live under `core/src/domain/`:

- **hardware** — `HardwareSnapshot` (CPU, RAM, GPU adapters, VRAM), `RuntimeProbe` (discovered `llama-server` runtime + devices), `RuntimeStatus`.
- **models** — `ModelId`, `ModelDefinition`, `ModelArtifact`, `InstalledModel`, `ModelCapabilities`, `ModelRuntimeHints`, `ModelSource`, `Quantization`.
- **profiles** — `TaskProfile`, `TaskKind`, `ReasoningMode`.
- **runtime** — `LaunchPlan`, `RuntimeState` (stopped/loading/ready/failure lifecycle), `RuntimeHandle`, semantic `RuntimeConfiguration`.
- **inference** — chat request/response/stream types and `InferenceError`.
- **metrics** — `InferenceMetric` (JSONL metric persistence support).
- **recommendation / configuration** — recommendation output types and `UserConfiguration` (model roots, server path).

## 6. Catalog / model / artifact / installed-model separation

**Implemented.**

- `ModelDefinition` = model identity and metadata (id, display name, family, architecture, parameters, context limit, roles, capabilities, default profile, sampling defaults, source).
- `ModelArtifact` = artifact metadata (id, quantization, filename, optional mmproj, optional sha256, runtime compatibility list).
- `InstalledModel` = actual local installation state (local path, mmproj path, verification flag, timestamps).
- `CatalogEntry` couples one definition with one artifact plus catalog-relative paths.
- `ModelCatalog::from_legacy_json` ingests the repository `models.json` (legacy camelCase schema) without retaining absolute user paths in catalog data.
- `ModelCatalog::installed_models(roots)` resolves relative artifact paths against user-configured model roots. Path safety: absolute paths, parent traversal, root, and Windows prefix components are rejected (`is_safe_relative_path`); artifacts outside a root are never exposed.
- `ModelResolver::resolve(model_id, runtime_id, ...)` returns a `ResolvedModel`, preferring the highest quantization rank among locally installed compatible artifacts. Scoring remains the recommendation engine's responsibility.

**Known limitations:**

- Artifact integrity verification (SHA-256) is implemented but currently unused: the catalog carries no `sha256` hashes, so `InstalledModel.verified` is always `false` today.
- `InstalledModel.installed_at_epoch_seconds` is a placeholder (`0`); installation timestamps are not tracked.
- The resolver's artifact choice among multiple installed quantizations is rank-based, not quality-model-based.

## 7. Hardware abstraction

**Implemented.**

- Portable CPU/RAM discovery via `sysinfo`.
- `llama-server --list-devices` parsing for runtime device discovery (order-aware: Vulkan0/Vulkan1-style ordering is preserved).
- Platform-aware native GPU probes: `nvidia-smi` on Windows/Linux, `system_profiler` on macOS, with runtime-device fallback.
- `hardware_tier()` derived metadata classifying the machine (entry / light-laptop / laptop / standard / high / enthusiast) to bound context sizes and batch policies.
- `HardwareSnapshot` feeds feasibility checks and the `doctor()` workflow.

**Known limitation:** tier classification is approximate starting policy, not benchmark-derived.

## 8. Recommendation pipeline

**Implemented.**

- Feasibility-first: capability, runtime compatibility, and resource (RAM/VRAM) checks gate scoring; infeasible models are rejected with reasons instead of scored.
- Explainable output: recommended model plus alternatives and rejections, each with reasons.
- `TaskProfile` (task kind, target context, reasoning preference, parallelism, low-latency, deterministic, cpu-only flags) drives the fit.
- Produces a semantic `RuntimeConfiguration` (context size, CPU threads, batch/ubatch, parallelism, KV cache type, prompt-cache ceiling, `--fit` safety margin, device preference, GPU-layer policy, host/port, model path) independent of llama.cpp CLI terminology.
- Model selection falls back to the catalog entry's `default_profile` when the user has not explicitly chosen one.

Non-goal for this baseline: modifying recommendation scoring or feasibility thresholds.

## 9. Application-service layer

**Implemented.** `core/src/application/service.rs` defines `LyonixLab`, the facade all consumers (CLI now, Tauri later) must use:

- `open(repo_root)` — loads the catalog, hardware snapshot, and user configuration (model roots and `llama_server_path` via the existing persistence mechanism).
- `doctor()` — `DiagnosticReport` (OS, hardware, runtimes, runtime status, recent log).
- `list_models()` — installed catalog models with local paths.
- `recommend(profile)` — recommendation with recommended model, semantic configuration, alternatives/rejections.
- `plan(model_id, config)` — `LaunchPlan`.
- `start(model_id, config, timeout)` — starts via `ProcessSupervisor`, waits for READY, persists durable runtime state; returns the managed process plus the persisted record.
- `wait_ready()` — health polling helper.
- `status()` — reconciles durable state with live health.
- `chat(request, timeout)` — blocking inference.
- `chat_stream(request, timeout, callback)` — streaming inference (`Delta`/`Done` events).
- `stop()` — stops through `ProcessSupervisor` (identity-verified against the persisted record) and clears the durable record.
- `run(profile, prompt, max_tokens, timeout)` — full lifecycle in one process: recommend → start → READY → inference → stop. Cleanup-safe: stop is attempted even when earlier steps fail.

Errors surface as typed `WorkflowError` values suitable for CLI exit codes. Business logic must live here, not in CLI/Tauri code.

## 10. Runtime adapter architecture

**Implemented (llama.cpp only).** The adapter translates the semantic `RuntimeConfiguration` into a runtime-specific `LaunchPlan` and keeps endpoint/CLI details behind the runtime boundary. Full details: `docs/RUNTIME-ADAPTERS.md`.

## 11. ProcessSupervisor and process ownership

**Implemented.** `core/src/runtime/supervisor.rs`:

- `ProcessSupervisor::start(&LaunchPlan)` spawns an owned child process, records PID + launch fingerprint identity, and creates per-runtime stdout/stderr log files under the state/log directories.
- `ManagedProcess` holds the `Child` handle — the ownership primitive.
- `ProcessSupervisor::stop(managed)` verifies `child.id() == handle.pid` before killing, then waits for exit. No name-based process search or killing.
- `ProcessSupervisor::stop_by_pid(pid, expected_executable)` serves short-lived CLI invocations that hold no child handle: the persisted PID and executable path are verified against the live process (via `sysinfo`) before the kill. An unrelated or already-exited process is refused (`OwnershipMismatch`), never touched.

**Known limitation:** `sysinfo`'s `exe()` can return `None` on access-restricted processes; the code treats that as `OwnershipMismatch` — a safe refusal, never a wrong kill.

## 12. Persistent runtime state

**Implemented.** `core/src/persistence/runtime_state.rs`:

- `PersistedRuntimeState` records: runtime id, PID, executable path, launch fingerprint, endpoint, model id, artifact id, start time, stdout/stderr log paths.
- Stored as JSON at `{state_dir}/runtime-state.json` (`%APPDATA%\LYONIX-LAB\state\runtime-state.json` on Windows), with `save` / `load` / `clear`.
- Purpose: durable state for later invocations. A CLI process is short-lived, so `status` / `chat` / `chat_stream` / `stop` reconcile from this record across separate processes.
- The persisted state does NOT replace `ProcessSupervisor` ownership checks — it is reconciliation state only. Stopping always goes through the supervisor with identity verification; stale records are cleared after stop or when the endpoint is unreachable.

## 13. Inference architecture

**Implemented.** `core/src/runtime/inference.rs`:

- `LlamaCppInferenceClient` (plus `from_endpoint` and the `single_turn_request(prompt, max_tokens)` convenience) over the managed localhost endpoint — `http://127.0.0.1:8081/v1` (chat completions at `/v1/chat/completions`).
- Blocking `chat()` → `ChatResponse`.
- Streaming `chat_stream()` — SSE parsing → `ChatStreamEvent::Delta { content }` and `ChatStreamEvent::Done { finish_reason }`.
- Typed `InferenceError`: `NotReady`, `InvalidRequest`, `NonLocalhost`, `Http`, `Io(#[from])`, `Status { code, message }` (carries the server message).
- HTTP/endpoint details stay behind the runtime/API boundary; the application layer never sees raw URLs.
- JSONL metric persistence (`InferenceMetric`) exists in the domain/persistence layer.

**Known limitations:** metrics are persisted but there is no aggregation/calibration engine; one model per runtime process.

## 14. CLI boundary

**Planned** (not built in this baseline). The application facade and durable runtime state are the CLI's foundation:

- A CLI must call only `LyonixLab` workflows — never orchestrate catalog/recommendation/runtime/inference logic itself.
- Cross-invocation `start` / `status` / `stop` relies on the persisted runtime state plus `stop_by_pid`; the `run` workflow owns the entire lifecycle in one process.

## 15. Future Tauri boundary

A Tauri 2 + Svelte 5 + TypeScript shell exists under `app/` from an earlier milestone (ADR-002) as a thin IPC client over typed commands (`get_hardware`, `get_runtimes`, `get_installed_models`, `get_runtime_status`, `refresh_snapshot`, `start_runtime`, `stop_runtime`), with a minimal capability set and no generic command executor.

**Status: exists; not the current core milestone.** It was checked structurally in the earlier milestone's environment; it has not been validated on this machine within the current baseline, and it does not yet consume the `LyonixLab` facade. The intended direction is that Tauri becomes a later consumer of the same application services.

## 16. Security boundaries

**Implemented.** Summary (full detail in `docs/SECURITY.md`):

- Localhost-only runtime binding (managed `127.0.0.1:8081`); the inference client refuses non-localhost endpoints (`InferenceError::NonLocalhost`).
- Process ownership via child handle + PID/fingerprint; name-based killing is prohibited.
- PID + executable identity validation in `stop_by_pid`.
- Model path safety: catalog-relative paths, traversal/absolute rejection, root confinement.
- Safe model-root handling: user configuration only; no machine paths in source or tests.
- Managed runtime arguments: port/host/model/context are adapter-owned; the `customArgs` escape hatch is restricted and cannot override managed settings or enable `--tools`.
- No generic command executor in the Tauri layer.

## 17. Configuration model

**Implemented.**

- User configuration: `{config_dir}/config.toml` — `%APPDATA%\LYONIX-LAB\config.toml` on Windows. Holds model roots and the `llama_server_path` override.
- `LYONIX_CONFIG_DIR` environment variable overrides the config/state directory location (used by tests for isolation).
- State directory: `{state_dir}` — `%APPDATA%\LYONIX-LAB\state` on Windows. Holds `runtime-state.json` and per-runtime logs.
- Model catalog: repository-root `models.json`; artifact paths are relative to configured model roots.
- `profiles.json` and `launcher.ini` / `launcher.user.ini` belong to the legacy PowerShell flow only.

Persistence errors are typed (`StateError`); no SQLite.

## 18. Extensibility model

- **New runtime:** implement the adapter translation (`RuntimeConfiguration` → `LaunchPlan`), health mapping, and supervisor integration; list the runtime id in catalog `runtime_compatibility`. See `docs/RUNTIME-ADAPTERS.md`.
- **New model:** add a catalog entry to `models.json` (definition + artifact + relative path) — no code changes required.
- **New profile:** extend `TaskProfile`/`TaskKind` in the domain layer.
- **New consumer (CLI/Tauri):** program against `LyonixLab` only.

## 19. Known architectural decisions

Established in the repository (see also `docs/ADR-001`, `docs/ADR-002`):

- **Feasibility before scoring** — infeasible models are rejected with reasons, never ranked.
- **ModelDefinition / ModelArtifact / InstalledModel separation** — identity, artifact metadata, and local installation state are distinct types.
- **Runtime-neutral application layer** — the facade and domain carry no llama.cpp-specific concepts.
- **ProcessSupervisor ownership** — every llama.cpp process is owned and identity-tracked; no raw `Command::new()` in production code.
- **Durable runtime state as reconciliation state, not ownership authority** — the persisted record enables cross-invocation status/stop; the supervisor remains the kill authority.
- **Localhost-only default** — managed `127.0.0.1:8081` binding; non-local endpoints refused.
- **llama.cpp as the first runtime** — the only implemented adapter.
- **SQLite deferred** — JSON/JSONL persistence at this stage.
- **Additional runtimes deferred** — Ollama/LM Studio/MLX/vLLM are future work.
- **Tauri as a later consumer of the same application services** — the UI is a presentation/IPC boundary, not an orchestration layer.

## 20. Known limitations

- Single crate with module layering; no workspace-level crate separation.
- Resolver artifact selection is quantization-rank-based only.
- SHA-256 artifact verification is implemented but unused (catalog carries no hashes).
- Installation timestamps are placeholders (`0`).
- `status()` reconciliation does not distinguish a crashed-but-recorded process from a cleanly stopped one (both report stopped/absent health); a `Failed`/`Crashed` reconciliation state would be a small follow-up.
- A port conflict fails safely (`PortInUse`) before spawning; callers must surface that error rather than retry.
- Metrics are persisted without an aggregation/calibration engine.
- The legacy PowerShell launcher remains in the repository as a reference path.
- The Tauri app under `app/` has not been validated on this machine in the current baseline.
- Windows is the validated platform; macOS/Linux GPU probes exist but are unvalidated.
- Feasibility thresholds and hardware tiers are policy, not benchmark claims.

## 21. Explicitly deferred decisions

- Ollama, LM Studio, MLX, and vLLM runtime adapters.
- Model download/install service (including checksum/signature enforcement on download).
- Managed runtime distribution / sidecar packaging.
- SQLite persistence.
- Benchmark execution/calibration engine.
- Production packaging, signing, and updater.
- Live log streaming to the UI.
- Persistent process recovery across application restarts (durable state enables reconciliation, not revival).
- Dynamic plugins.
- Shell access via a narrowly scoped capability (only if a concrete use case requires it).
