# LYONIX-LAB Security Model

Security boundaries as implemented in the repository as of Core Architecture Baseline v1.

## Localhost-only runtime boundary

**Implemented.** The managed runtime always binds to `127.0.0.1:8081` (canonical port, chosen to avoid collisions with 8080). The host/port are adapter-managed arguments, not user-suppliable values.

The inference client enforces this independently: `LlamaCppInferenceClient` refuses non-localhost endpoints with `InferenceError::NonLocalhost`, so no code path through the application layer can point inference at a remote host. Arbitrary HTTP URLs are not exposed to the UI or CLI; consumers only ever see typed workflow results.

## Process ownership

**Implemented.** Every `llama-server` process is spawned by `ProcessSupervisor` as an owned child:

- The `Child` handle in `ManagedProcess` is the ownership primitive.
- `ProcessSupervisor::stop(managed)` verifies `child.id() == handle.pid` before killing, then waits for exit.
- Production code never spawns the runtime with raw `Command::new()` outside the supervisor.
- Spawn identity (PID + launch fingerprint) is recorded at start; stdout/stderr are redirected to per-runtime log files under the state directory.

## PID + executable identity validation

**Implemented.** Short-lived CLI processes hold no child handle, so stopping a previously started runtime goes through `ProcessSupervisor::stop_by_pid(pid, expected_executable)`:

1. The persisted runtime-state record supplies PID and executable path.
2. The live process table (`sysinfo`) is consulted: the process must exist, its PID must match, and its executable path must match the persisted identity.
3. Only then is the kill issued; termination is verified by re-refreshing the process table.
4. Any mismatch — unrelated process, already-exited PID, PID reuse with a different executable, or an `exe()` that cannot be read — results in `SupervisorError::OwnershipMismatch` and no kill.

**Known limitation:** `sysinfo`'s `exe()` can return `None` on access-restricted processes. The code treats that as a safe refusal (never a wrong kill); the practical consequence is that a stop may be refused and require manual investigation rather than silently succeeding.

## Why name-based process killing is prohibited

Name-based killing (`taskkill /IM llama-server.exe`-style or searching process lists by executable name) cannot distinguish the runtime this application owns from:

- another copy the user started manually,
- a copy another tool (e.g. a different local-LLM manager) started,
- an unrelated process that happens to share the executable name.

Killing by name could terminate a user's own workload. LYONIX-LAB therefore only ever kills a process whose identity (PID + executable path +, in the in-process case, the child handle itself) was established at spawn time and re-verified before the kill. This rule is enforced in code (`stop`, `stop_by_pid`) and documented as an invariant for future contributors.

## Model path safety

**Implemented.** Model artifact paths in the catalog are relative to configured model roots. `ModelCatalog::installed_models` and the path-safety helper enforce:

- absolute catalog paths are rejected,
- parent traversal (`..`) is rejected,
- root and Windows prefix components are rejected,
- artifacts are only ever resolved inside a configured root,
- an unsafe path causes the entry to be skipped, never a fallback to an arbitrary location.

Root confinement is not weakened for tests: tests use temporary directories or the real user configuration, not hardcoded machine paths.

## Safe model-root handling

**Implemented.** Model roots come exclusively from user configuration (`%APPDATA%\LYONIX-LAB\config.toml`, `model_roots = [...]` string array with `version = 1`), overridable for tests via `LYONIX_CONFIG_DIR`. No machine-specific absolute paths are embedded in Rust source or in the catalog data. The repository `models.json` records only relative paths plus metadata.

## Managed runtime arguments

**Implemented.** The llama.cpp adapter owns the security-relevant arguments:

- host/port (`127.0.0.1:8081`),
- model path (resolved inside a configured root),
- context size and memory policy,
- device selection and GPU-layer policy,
- reasoning controls.

These are translated from the semantic `RuntimeConfiguration` and cannot be overridden by user-supplied strings.

## Restricted raw runtime arguments

**Implemented.** A restricted `customArgs` escape hatch exists for advanced users, but it cannot:

- override managed process/network/model settings (`--port`, `--host`, `--model`, `--ctx-size`, `--n-gpu-layers`, `--device`),
- enable llama.cpp built-in tools (`--tools`),

and conflicting arguments are rejected/warned rather than silently applied. Host tools remain disabled by default; the external application stays the tool owner.

## Future security considerations for runtime installation/downloads

**Deferred.** Model download/installation is not implemented. When it is, it must address at minimum:

- checksum (SHA-256) and signature verification before an artifact is trusted — the `InstalledModel.verified` field and SHA-256 helper already exist for this purpose,
- download source allowlisting and HTTPS enforcement,
- sandboxed extraction and path-safety re-validation after extraction,
- disk-quota and malware-scan policy for third-party GGUF content,
- managed runtime distribution (sidecar binaries) would additionally require binary signing and update-channel integrity.

None of these exist today; this section is forward-looking documentation, not a claim of implementation.

## Non-security summary

Out of scope for this baseline: remote inference, multi-tenancy, authentication on the local endpoint (the runtime binds to loopback only and is intended for single-user local use), and encryption of local state files.
