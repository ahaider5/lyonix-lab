# Runtime Adapter Architecture

The adapter boundary as implemented in the repository as of Core Architecture Baseline v1. Exactly one adapter exists today: **llama.cpp**. Ollama, LM Studio, MLX, vLLM, and other runtimes do not exist in this repository and are explicitly deferred.

## The RuntimeAdapter boundary

The adapter contract sits between the runtime-neutral application layer and a concrete inference engine. Its job is translation and lifecycle integration — never orchestration.

### Responsibilities

1. Translate the semantic `RuntimeConfiguration` into a runtime-specific `LaunchPlan` (executable, arguments, environment, working directory).
2. Map runtime lifecycle states onto the shared `RuntimeState` lifecycle (stopped / loading / ready / failure).
3. Keep runtime-specific HTTP/endpoint/CLI details behind the boundary: the application layer never constructs URLs or raw CLI strings.
4. Provide the managed arguments (host/port, model path, context, memory policy, device policy) that users cannot override.
5. Enforce the restricted `customArgs` escape hatch (no override of managed settings, no `--tools`).

### Inputs

- A semantic `RuntimeConfiguration` (context size, CPU threads, batch/ubatch, parallelism, KV cache type, prompt-cache ceiling, `--fit` safety margin, device preference, GPU-layer policy, host/port, resolved model path) produced by the recommendation pipeline.
- A `ResolvedModel` (definition + artifact + installed local path) from the `ModelResolver`.
- Optional user-supplied restricted `customArgs`.

### Outputs

- A `LaunchPlan` consumed by `ProcessSupervisor::start`.
- Health mapping used by the runtime health module (`/health`-style polling → `RuntimeState`).
- Endpoint identity (`http://127.0.0.1:8081/v1` for llama.cpp) consumed by the inference client — but only behind the API boundary.

### Capabilities

- Localhost-only managed networking.
- Owned-process supervision integration (PID/fingerprint identity, per-runtime logs).
- Automatic GPU-layer policy (`-ngl auto` + `--fit on`) rather than hard-coded layer counts.
- Reasoning controls mapped from `ReasoningMode`.
- Streaming-capable endpoint (SSE chat completions).

## LaunchPlan

`LaunchPlan` (domain type) is the single hand-off point between configuration and process supervision:

- executable path (resolved `llama-server`),
- full argument vector (managed arguments first, restricted extras last),
- environment and log file locations,
- endpoint/port identity for later health and inference use.

`ProcessSupervisor` spawns exactly what the plan says, records PID + launch fingerprint, and creates the stdout/stderr log files. Nothing else in the codebase spawns the runtime.

## Runtime-specific translation

Semantic configuration → llama.cpp arguments is a pure, testable translation. Examples of the mapping policy:

- context size → `--ctx-size` (bounded by the hardware tier),
- CPU threads → `--threads` / `--threads-batch`,
- batch/ubatch → `--batch-size` / `--ubatch-size`,
- parallelism → `--parallel`,
- KV cache type → `--cache-type-k` / `--cache-type-v` (architecture-aware hints from the catalog),
- prompt-cache ceiling → `--cache-ram`,
- `--fit` safety margin → `--fit on`,
- device preference → `--device` with llama.cpp device names from `--list-devices`,
- GPU layers → `-ngl auto` when a dedicated GPU is available.

This is deliberately policy-based: one static `-ngl` number is not optimal on every machine, and GPU-layer allocation ultimately remains with llama.cpp's runtime allocator.

## llama.cpp implementation

**Implemented.** `core/src/runtime/llama_cpp/`:

- `RuntimeConfiguration` → `LaunchPlan` translation (above).
- Health endpoint polling (`/health`) with state mapping; the validated lifecycle is start → loading → READY → failure-on-exit.
- Managed defaults: `127.0.0.1:8081`, model from the resolved installed path, built-in host tools disabled.
- Restricted `customArgs` validation.
- Validated on the real development machine (Windows, i5-10210U, MX250, Vulkan backends) with the full lifecycle and both blocking and streaming inference.

The llama.cpp inference client (`core/src/runtime/inference.rs`) implements the chat protocol against the managed endpoint: blocking `chat()` and streaming `chat_stream()` with SSE parsing into `Delta`/`Done` events, typed `InferenceError`s, and a non-localhost refusal.

## What future adapters would need to provide

**Deferred — documentation only, none of these exist today.** A new runtime (e.g. Ollama, LM Studio, MLX, vLLM) would need to provide:

1. A translation from `RuntimeConfiguration` to that runtime's `LaunchPlan` (or an equivalent spawn description), including its own managed-argument policy.
2. Health/readiness mapping onto `RuntimeState` (its own endpoint or process-state probe).
3. Endpoint identity for its inference client, still localhost-only and still behind the API boundary.
4. A chat/inference protocol implementation (or a translation to the shared chat types).
5. Streaming support or an explicit statement that the runtime cannot stream cleanly.
6. Catalog `runtime_compatibility` entries (`"ollama"`, etc.) so the resolver can match artifacts to the runtime.
7. Compliance with the security model: owned processes, no name-based killing, no unrestricted argument strings, no remote endpoints.

Adding an adapter must not weaken feasibility checks or modify the recommendation pipeline; the common behavior is validated against a real adapter before additional interfaces are accepted.
