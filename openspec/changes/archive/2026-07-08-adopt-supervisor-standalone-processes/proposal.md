## Why

Standalone long-running processes (`tedge-agent`, `tedge-mapper`) currently use a non-reloadable logging subscriber and a `SignalActor` that only handles SIGINT/SIGTERM/SIGQUIT. Sending SIGHUP to a standalone process terminates it (default OS behaviour). The supervisor already supports SIGHUP-driven log level reloading from `system.toml`, crash isolation with backoff restarts, and graceful shutdown — but these capabilities are only available when running via `tedge run all`. Adopting the supervisor for standalone processes unifies signal handling and makes SIGHUP log level reloading available regardless of deployment mode.

## What Changes

- Standalone `tedge-agent` and `tedge-mapper` run through the supervisor with a single unit, gaining SIGHUP log level reloading and crash-resilient restarts
- Standalone processes use `log_init_reloadable_for_services()` instead of the non-reloadable `log_init()`, enabling runtime log level changes via `system.toml`
- The `SignalActor` is no longer used by long-running processes; the supervisor's signal listener handles SIGINT/SIGTERM/SIGQUIT/SIGHUP uniformly
- SIGUSR1 (mapper restart) is not supported for standalone processes, as it is not meaningful outside `tedge run all` where there is a coordinated multi-component lifecycle

## Capabilities

### New Capabilities

- `standalone-supervisor`: Running standalone long-running processes (tedge-agent, tedge-mapper) through the supervisor with a single unit, providing SIGHUP log level reloading and crash recovery

### Modified Capabilities

- `single-process-supervisor`: The supervisor's scope expands from `tedge run all` only to also cover standalone process execution with a single unit

## Impact

- `crates/core/tedge_agent/src/lib.rs` — standalone `run()` path changes to use the supervisor
- `crates/core/tedge_mapper/src/lib.rs` — standalone `run()` path changes to use the supervisor
- `crates/common/tedge_supervisor/` — new crate extracted from the supervisor in `crates/core/tedge/`, providing `Supervisor`, `Unit`, `UnitKind`, `RuntimeFactory`, and `run_standalone()` for use by all three entry points
- `crates/core/tedge/src/supervisor.rs` — now imports from `tedge_supervisor` and wires up the multi-unit `tedge run all` path
- `crates/extensions/tedge_signal_ext/` — `SignalActor` is no longer used by long-running processes (still used by short-lived CLI commands if applicable)
- systemd service files — no change expected; the process entry point remains the same
