## Context

Long-running thin-edge.io processes have two execution paths:

1. **Supervisor path** (`tedge run all`): The `Supervisor` in the `tedge_supervisor` crate (`crates/common/tedge_supervisor/`) wraps components as `Unit`s, owns all signal handling (SIGINT/SIGTERM/SIGQUIT/SIGUSR1/SIGHUP), initialises a reloadable tracing subscriber via `log_init_reloadable_for_services()`, and drives `run_to_completion_supervised()` which returns errors instead of calling `process::exit`.

2. **Standalone path** (`tedge-agent`, `tedge-mapper c8y`): Each process calls `log_init()` (non-reloadable), acquires its own lock, spawns a `SignalActor` (SIGINT/SIGTERM/SIGQUIT only), and calls `run_to_completion()` which exits the process on error. SIGHUP terminates the process (default OS behaviour).

Both paths share the same `build()` factory for constructing the actor runtime. The difference is what wraps the runtime after construction.

## Goals / Non-Goals

**Goals:**

- Standalone long-running processes use the supervisor with a single unit, gaining SIGHUP log level reloading and crash-resilient restarts
- Reuse the existing `Supervisor` and `log_init_reloadable_for_services()` rather than duplicating signal/reload logic
- Standalone behaviour remains unchanged from the operator's perspective: same binary, same CLI, same systemd integration

**Non-Goals:**

- SIGUSR1 mapper restart for standalone processes — this signal is meaningful only when the agent and mapper share a process, where it allows restarting the mapper without restarting the agent. A standalone mapper has nothing to coordinate with.
- Changing the `build()` factory contract — standalone and `tedge run all` paths continue to share the same factory
- Replacing systemd restart semantics — the supervisor's crash recovery complements systemd, it does not replace it

## Decisions

### Decision 1: Reuse `Supervisor` with a single `Unit`

The standalone `run()` functions in `tedge_agent` and `tedge_mapper` will construct a `Supervisor` with exactly one `Unit` and call `Supervisor::run()`, rather than manually wiring `SignalActor` + `run_to_completion()`.

This reuses all existing supervisor infrastructure: signal listener, command loop, backoff restarts, log reload, and graceful shutdown. The `SignalActor` is no longer spawned by long-running processes.

**Alternative considered — Extract SIGHUP handling into a shared function used by both paths:** This would duplicate the signal listener setup, the command channel, and the reload-vs-shutdown coordination. The supervisor already encapsulates all of this cleanly.

### Decision 2: SIGUSR1 is ignored for standalone processes

The signal listener will still register SIGUSR1 (it is process-wide), but `restart_mappers()` is a no-op when the supervisor contains no mapper units (standalone agent) or a single mapper unit that has no coordination benefit from a signal-driven restart. A standalone mapper receives SIGUSR1 the same as a crash-restart — the supervisor's existing `restart_units()` handles it — but there is no user-facing reason to advertise this signal for standalone use.

The signal listener can remain unchanged; the supervisor naturally handles the empty-selector case.

### Decision 3: Move log initialisation into the supervisor entry point

The standalone `run()` functions currently call `log_init()`. After this change, they will call `log_init_reloadable_for_services()` instead, passing a single-element service name list. The returned `LogLevelReloadHandle` is passed to the supervisor via `with_log_reload()`.

This is the same pattern `tedge run all` already uses, just with fewer service names.

### Decision 4: Extract the supervisor into its own crate

The supervisor was extracted from `crates/core/tedge/src/supervisor.rs` into a standalone crate `tedge_supervisor` (`crates/common/tedge_supervisor/`). This avoids cyclic dependencies: both `tedge_agent` and `tedge_mapper` need the supervisor, but the `tedge` crate depends on both of them, so re-exporting from `tedge` would create a cycle.

Both `tedge_agent::run()` and `tedge_mapper::run()` follow the same pattern: acquire lock → build reloadable logger → call `Supervisor::run_standalone()`. The `tedge run all` path continues using the multi-unit construction it already has.

### Decision 5: Standalone `run_to_completion()` is replaced by `run_to_completion_supervised()`

The supervisor always uses `run_to_completion_supervised()` (which returns errors) rather than `run_to_completion()` (which calls `process::exit`). The supervisor's own exit handling — logging the error and either restarting or shutting down — replaces the direct `process::exit` call.

When the supervisor itself finishes (all units stopped, or shutdown completed), the standalone `run()` function returns the result normally, and the process exits through the standard `main()` return path.

## Risks / Trade-offs

**[Crash restart under systemd]** → The supervisor's restart policy runs inside the process, while systemd also has restart-on-failure. A component that repeatedly crashes will be restarted by the supervisor first; only if the supervisor itself exits (which it currently does not on unit exhaustion) would systemd intervene. This is complementary: the in-process restart is faster (no process spawn overhead) and preserves the reloadable logger and lock state. Mitigation: document that the supervisor's restart policy runs first, and systemd's `Restart=on-failure` serves as a second layer.

**[SIGHUP sent before supervisor is ready]** → If a SIGHUP arrives between process start and the signal listener registration, it will terminate the process (default behaviour). This is the same race that exists today in the `tedge run all` path and is inherent to async signal registration. Mitigation: signal registration happens early in `Supervisor::run()`, before any unit is spawned.

**[Single-unit supervisor overhead]** → The supervisor's event loop, command channel, and backoff infrastructure are lightweight (no threads, no allocations in the steady state). The overhead of running a single unit through this loop is negligible compared to the component's own actor runtime.
