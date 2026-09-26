# LYONIX-LAB Implementation Status

## Phase A / B

Complete before implementation. See the approved architecture/research in the project task materials.

## Phase C / D decisions implemented

- Modular Rust core
- ModelDefinition / ModelArtifact / InstalledModel separation
- Explicit ModelResolver
- Feasibility before scoring
- Hardware tier retained as derived metadata
- Typed RuntimeConfiguration
- llama.cpp as the only implemented runtime
- Owned managed-process lifecycle primitives
- Localhost-first network policy
- Built-in runtime tools disabled
- Restricted raw runtime argument escape hatch
- JSONL metrics; no SQLite yet
- Persistent per-runtime stdout/stderr logs
- Platform-aware native GPU probes (NVIDIA nvidia-smi on Windows/Linux; Apple system_profiler on macOS) with runtime-device fallback
- User model-root persistence and diagnostics export

## Phase E implemented in this milestone

- Tauri 2 shell
- Svelte 5 + TypeScript + Vite frontend
- Typed Tauri IPC boundary
- Dashboard / Models / Runtime / Profiles / Settings / Diagnostics views
- Backend-only process execution
- Runtime health polling via IPC
- Embedded model/profile defaults without writing repository files at runtime
- Enriched model catalog metadata: roles, capabilities, per-model default profiles, recommended context, and architecture-aware KV hints
- Model selection now falls back to the selected model's default profile when the user has not explicitly chosen one.
- Minimal Tauri capability set (`core:default` only)
- CSP restricted to local assets + Tauri IPC

## Not implemented yet

- Live log streaming
- Model download service
- Managed runtime distribution
- Ollama adapter
- LM Studio adapter
- benchmark execution/calibration UI
- SQLite persistence
- production packaging/signing/updater

## Validation note

The execution environment used for this migration does not contain Cargo/Rust or an npm package cache. The project manifests and source were checked structurally, but a local `cargo check` and `npm run build` could not be executed here. `npm install` also could not complete because the environment could not reach the npm registry.
