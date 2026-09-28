# LYONIX-LAB Agent Instructions

## Mission

LYONIX-LAB is a hardware-aware local AI runtime/control plane.

The repository prioritizes:
- correctness
- architectural boundaries
- security
- explainability
- testability
- incremental delivery

## Source of truth

Before making architectural changes, read:

- README.md
- docs/ARCHITECTURE.md
- docs/ROADMAP.md
- docs/DEVELOPMENT.md
- docs/SECURITY.md
- docs/RUNTIME-ADAPTERS.md
- CONTRIBUTING.md

Do not contradict documented architecture without explicitly
calling out the architectural change.

## Architecture rules

- Keep domain types runtime-neutral.
- CLI and Tauri must consume application services.
- Do not put business logic in CLI commands.
- Do not put business logic in Tauri commands.
- Runtime-specific CLI arguments belong in runtime adapters.
- Process ownership belongs to ProcessSupervisor.
- Never kill processes by name.
- Recommendation must perform feasibility before scoring.
- Preserve ModelDefinition / ModelArtifact / InstalledModel separation.
- Do not bypass path-safety checks.
- Do not expose inference remotely by default.
- llama.cpp is currently the only implemented runtime.

## Development workflow

Before editing:
1. Inspect relevant architecture and implementation.
2. State the intended change.
3. Identify affected modules.
4. Identify tests.

After editing:
1. Run targeted tests.
2. Run cargo check.
3. Run cargo test.
4. Run cargo check --all-targets.
5. Run clippy with -D warnings.
6. Report what changed and what was verified.

Do not run `cargo fmt` globally unless explicitly requested.

## Scope discipline

- Do not refactor unrelated code.
- Do not modify architecture merely to simplify the current task.
- Do not introduce a dependency unless justified.
- Do not implement deferred roadmap features as part of another task.
- Do not silently change public behavior.

## Security

- Localhost-only inference is the default.
- Managed runtime arguments cannot be overridden by arbitrary user arguments.
- Built-in runtime tools remain disabled by default.
- Never commit secrets, API keys, local configuration, models, logs,
  runtime state, or machine-specific paths.

## Completion criteria

A task is not complete because code compiles.

It is complete when:
- the requested behavior works,
- tests cover the important path,
- architectural boundaries remain intact,
- validation passes,
- and the implementation is documented when appropriate.

## Agent orchestration and worker integration

The current repository checkout is the authoritative LYONIX-LAB repository.

### Parent orchestrator

The main Hermes session is the project orchestrator.

It is responsible for:
- planning the current implementation slice;
- delegating bounded tasks to workers;
- reviewing worker results;
- running or coordinating validation;
- integrating accepted worker changes into the LYONIX-LAB repository;
- updating project state.

### Worker roles

- **Delegated Hermes:** analysis, architecture review, investigation,
  test planning, and independent verification. It should not modify
  project code unless explicitly assigned an implementation task.
- **OpenCode:** primary implementation worker for bounded coding tasks.

### Worker isolation

Workers may use isolated Git worktrees or work directories.

Workers must:
- never initialize a new Git repository;
- treat their checkout as a temporary implementation workspace;
- make changes only on the assigned isolated branch/worktree;
- commit implementation work when instructed;
- report files changed, tests run, validation results, and remaining risks.

### Worker integration

Worker completion does not equal feature completion.

After a worker reports completion, the parent orchestrator must:

1. inspect the worker branch/worktree;
2. review the diff and commits;
3. verify that the changes satisfy the assigned task;
4. run the required validation;
5. resolve or reject incomplete work;
6. integrate accepted changes into the appropriate LYONIX-LAB branch;
7. verify the resulting repository state.

The final feature must exist in the LYONIX-LAB Git repository history.

Do not consider isolated worker changes complete merely because they
exist in a separate directory.

### Parallel work

- Never allow two workers to edit the same working tree simultaneously.
- Prefer one implementation worker per feature unless the task is
  genuinely separable.
- Parallel workers must use separate branches/worktrees.
- The parent orchestrator owns final integration.

### Merge authority

The parent orchestrator may merge a worker's branch when:
- the task is within the approved scope;
- architecture and security boundaries are unchanged;
- required validation passes.

Human approval is required before integrating changes that:
- alter documented architecture;
- change security boundaries;
- change persistence architecture;
- introduce a major dependency;
- add a new runtime;
- materially change product scope.