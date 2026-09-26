# ADR-001: Headless Rust Core Before Tauri UI

## Status

Accepted

## Context

LYONIX-LAB is migrating from a PowerShell launcher into a small cross-platform local-LLM manager. The main architectural risks are business logic leaking into the GUI, direct shell execution from the frontend, llama.cpp-specific concepts becoming the domain model, and premature support for multiple runtimes.

## Decision

Build the first implementation milestone as a headless Rust core. Keep the legacy PowerShell launcher intact during migration. Add Tauri 2 + Svelte only after the core contracts are exercised.

The core uses:

- normalized hardware domain types
- ModelDefinition / ModelArtifact / InstalledModel separation
- an explicit ModelResolver
- feasibility checks before scoring
- semantic RuntimeConfiguration
- a small RuntimeAdapter contract
- llama.cpp as the first and only runtime implementation
- owned managed-process supervision
- localhost-only networking by default
- no unrestricted custom argument string
- no default llama.cpp built-in tools
- JSONL metrics rather than SQLite at this stage

## Consequences

The first slice is easy to test without a GUI and does not require a large rewrite of the working launcher. The UI becomes a presentation/client boundary over tested backend operations. Additional runtimes can be added after the common behavior is validated against a real adapter rather than hypothetical interfaces.
