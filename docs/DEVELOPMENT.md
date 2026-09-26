# LYONIX-LAB Development Guide

Development documentation for the headless Rust core as of Core Architecture Baseline v1.

## Prerequisites

- **Windows 10/11** (the validated platform; macOS/Linux compile but GPU probes are unvalidated).
- **Rust toolchain**: a stable Rust compatible with the declared `rust-version` in `core/Cargo.toml`. The validated environment used Rust 1.8x with Clang 20.1.8 available via the MSVC toolchain. Check with `cargo --version` and `rustc --version`.
- **llama-server**: a recent llama.cpp build installed and discoverable on `PATH` (or pointed to via user configuration, see below). The validated build was `llama-server 0.4.1-dev`. It must support the options used by this project: `--fit`, `--device`, `--flash-attn auto`, `--cache-ram`, `--cache-type-k/v`, and reasoning controls.
- Model GGUF files referenced by `models.json` under a configured model root.

## Toolchain expectations

- Build/test commands are run from the `core/` directory.
- `cargo clippy --all-targets --all-features -- -D warnings` must stay clean — zero warnings, no `#[allow]` suppressions.
- Do not run `cargo fmt` globally (the project is not fmt-formatted by convention).

## Windows development setup

1. Clone the repository.
2. `cd core`
3. `cargo check` — first build.
4. Create the user configuration (see "Configuration locations" below) so the core can find your model root and `llama-server`.

PowerShell is only required for the legacy launcher (`launcher.ps1` / `diagnose.ps1`), which is a legacy reference path, not part of the core build.

## llama-server requirement

The runtime is llama.cpp (`llama-server`). The core:

- discovers it via user configuration (`llama_server_path`) or `PATH`,
- launches it as an owned supervised process on `127.0.0.1:8081`,
- polls `/health` until READY,
- never launches it from a normal `cargo test` run.

Run `llama-server --list-devices` once to confirm the backend sees your GPU(s). llama.cpp device ordering (e.g. Vulkan0/Vulkan1) is preserved by the core's device parsing.

## Building

```text
cd core
cargo check
cargo check --all-targets
```

The Tauri app under `app/` is a separate milestone (not the current core milestone); it is not required for core development.

## Testing

```text
cd core
cargo test
```

- 25 tests pass as of Baseline v1.
- Normal `cargo test` runs are safe: they never launch `llama-server`. Both real-machine smoke tests skip unless the gate variable is set.

## Real-machine smoke tests

Two smoke tests are gated behind `LYONIX_RUNTIME_SMOKE=1` and exercise the real lifecycle against the real configured model root:

- Inference + lifecycle smoke (resolve Spark → start llama.cpp → wait READY → tiny deterministic prompt → non-empty response → stop → port 8081 closed):

  ```text
  cd core
  LYONIX_RUNTIME_SMOKE=1 cargo test --test core_tests -- --nocapture --test-threads=1
  ```

- Streaming smoke (through the application facade: recommend → start → status reconciles Ready → streaming request → multiple Delta events → final Done → stop → port 8081 closed):

  ```text
  cd core
  LYONIX_RUNTIME_SMOKE=1 cargo test --test core_tests runtime_service_streaming_smoke -- --nocapture --test-threads=1
  ```

Run them one at a time (`--test-threads=1`): each owns port 8081. A run takes roughly 10–30 seconds depending on hardware.

## Configuration locations

Per-user, outside the repository:

- User configuration: `%APPDATA%\LYONIX-LAB\config.toml`
- State directory: `%APPDATA%\LYONIX-LAB\state\` — contains `runtime-state.json` and per-runtime stdout/stderr logs.
- `LYONIX_CONFIG_DIR` overrides the config/state directory root (used by tests for isolation; not normally needed by hand).

## Model-root configuration

Model roots are user configuration, never compile-time defaults:

1. Create or edit `%APPDATA%\LYONIX-LAB\config.toml` and point a model root at your model-library directory, e.g.:

   ```toml
   version = 1
   model_roots = ['D:\path\to\your\model\library']
   ```

2. `models.json` in the repository records each model's path relative to that root.
3. Discovery resolves the relative artifact paths under each configured root; paths attempting traversal or absolute paths in the catalog are rejected.

The repository is intentionally configured for three local models (Spark X2.5 1.7B Abliterated, Qwen3.5 2B Claude 4.6 HERETIC i1, Qwen2.5 Coder 1.5B Instruct). The model binaries are not stored in this repository.

## Contributor workflow

1. Branch from the default branch.
2. Make changes in the layer the change belongs to (see `CONTRIBUTING.md`).
3. Validate: `cargo check`, `cargo test`, `cargo check --all-targets`, `cargo clippy --all-targets --all-features -- -D warnings`.
4. Only run smoke tests when the change touches runtime lifecycle behavior.
5. Document behavior changes in `CHANGELOG.md` and, where architectural, in `docs/`.

## Debugging / log locations

- Per-runtime stdout/stderr logs: `%APPDATA%\LYONIX-LAB\state\` (paths are also reported in the persisted `runtime-state.json` and printed by the smoke tests).
- Durable runtime state: `%APPDATA%\LYONIX-LAB\state\runtime-state.json` — shows pid, endpoint, model/artifact id, and log paths for the last started runtime; cleared after a clean stop.
- If a runtime fails to start: check the log files first, then `llama-server --list-devices` for backend/device issues, then the port (`netstat -ano | findstr :8081`).
- The `doctor()` workflow (exposed via the application facade) reports OS, hardware, discovered runtimes, runtime status, and the recent log path.
