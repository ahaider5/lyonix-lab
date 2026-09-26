# LYONIX-LAB Rust Core

First implementation slice of the approved architecture. This package is intentionally headless; Tauri/Svelte is a later layer over this core.

## Implemented

- typed domain models for hardware, models, artifacts, installed models, profiles, runtime state/configuration, recommendations, and metrics
- portable CPU/RAM discovery via `sysinfo`
- llama.cpp runtime-device parsing from `llama-server --list-devices`
- enriched `models.json` catalog ingestion without keeping absolute user paths in catalog data
- model roles/capabilities/default-profile metadata and architecture-aware KV hints
- explicit `ModelResolver`
- capability/compatibility/resource feasibility before recommendation scoring
- explainable recommendation output with alternatives/rejections
- semantic runtime configuration independent of llama.cpp CLI terminology
- llama.cpp launch-plan translation
- localhost-only initial network policy with `127.0.0.1:8081` defaults
- llama.cpp `--fit` / automatic GPU-layer policy rather than hard-coded layer counts
- built-in llama.cpp tools disabled by default
- restricted raw runtime-argument escape hatch that cannot override managed process/network/model settings or enable `--tools`
- owned-process supervisor primitive with PID/fingerprint identity
- `/health`-style lifecycle states: loading/ready/failure/unknown
- JSONL metric persistence and SHA-256 helper
- unit/fixture tests for device ordering, feasibility, reasoning defaults, resolver selection, tool safety, and network policy

## Deliberately deferred

- Tauri 2 + Svelte UI
- native per-OS GPU inventory beyond the runtime execution view
- model download/install service
- SQLite
- Ollama/LM Studio adapters
- dynamic plugins
- managed runtime distribution/sidecars
- persistent process recovery across application restarts
- benchmark execution/calibration engine
