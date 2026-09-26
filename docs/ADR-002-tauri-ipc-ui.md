# ADR-002: Tauri UI as a Thin IPC Client

## Status

Accepted

## Context

The headless Rust core now owns hardware detection, model resolution, recommendation logic, runtime configuration, and managed-process lifecycle. The next milestone adds a desktop UI without moving business logic into components.

## Decision

Use Tauri 2 with Svelte 5 + TypeScript + Vite. The frontend talks only to explicitly defined Tauri commands. No Tauri shell/process plugin is exposed to the webview in this milestone.

The initial commands are:

- `get_hardware`
- `get_runtimes`
- `get_installed_models`
- `get_runtime_status`
- `refresh_snapshot`
- `start_runtime`
- `stop_runtime`

Runtime process execution remains backend-only. The main capability grants only the Tauri core default permission set; there is no generic command executor.

Tauri's current security model routes frontend requests through the runtime authority and capability layer, and its CSP guidance recommends allowing the IPC origin explicitly. The application therefore uses a minimal CSP with `connect-src ipc: http://ipc.localhost` and no remote content sources.

## Consequences

The frontend remains replaceable and testable as a presentation layer. The backend API becomes the stable contract for a future richer GUI. Shell access can be introduced later only with a narrowly scoped capability if a concrete use case requires it.
