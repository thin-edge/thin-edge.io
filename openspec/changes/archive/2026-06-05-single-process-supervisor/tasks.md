# Tasks

## 1. Refactor component entry points into a shared `build()`

- [x] Extract a shared assembly function for the **agent** (e.g. `build(opt, config) -> (RuntimeHandle, completion_future)`) that wires actors and spawns the runtime but neither blocks to completion nor exits the process.
- [x] Extract the equivalent shared assembly function for the **mapper**, mirroring the agent's signature so the supervisor can treat them uniformly.
- [x] Reimplement the standalone `run()` for both components as a thin wrapper over `build()` that preserves today's semantics exactly: `std::process::exit(1)` on runtime error, its own `SignalActor`, its own flockfile, and its own MQTT connection.
- [x] Confirm the `main.rs` multicall arms (`tedge-mapper c8y`, `tedge-agent`, …) are untouched and dispatch to the standalone wrappers.
- [x] Regression-check standalone behaviour: `tedge-mapper c8y` and `tedge-agent` start, log, handle SIGTERM, and exit on error bit-for-bit as before.

## 2. Make the runtime safe to supervise

- [x] Add a completion path on `Runtime` for supervised use that returns the runtime error to the caller instead of calling `std::process::exit`.
- [x] Confirm `panic = "unwind"` remains the release profile setting and add a comment/guard noting that crash isolation depends on it. (Comment in `Cargo.toml`; compile-time `#[cfg(panic = "abort")] compile_error!` guard in `supervisor.rs`.)
- [x] Ensure a unit's `build()` can be called repeatedly to produce a fresh incarnation (the rebuildable factory contract), with no lingering global state between incarnations. (Lock acquisition moved out of the factory; the one remaining global, `TEDGE_CLOUD_PROFILE`, is the documented single-mapper limitation.)

## 3. Supervisor core and `tedge run all`

- [x] Add the `tedge run all` multicall arm as the supervisor entry point, leaving existing arms unchanged.
- [x] Model a supervised **unit** as: name, kind (agent/mapper), rebuildable factory, graceful shutdown handle, current task handle, and restart policy.
- [x] Implement the supervisor loop: spawn each unit, `select!` over their completion, and react to a unit finishing (clean exit vs error vs panic).
- [x] Implement restart-on-crash with a bounded exponential backoff and a maximum number of attempts within a window; after the cap, stop restarting that unit and log that it has given up — without exiting the process.
- [x] Implement best-effort start ordering: spawn the agent before mappers, stop in reverse order, with **no** readiness gate — a mapper must start even if the agent is slow or absent.
- [x] Implement collective drain on shutdown: request all units to stop and wait for them within the runtime's existing cleanup timeout before the process exits.

## 4. Central signal ownership and restart control

- [x] Remove per-unit `SignalActor`s from the supervised path; register a single process-wide signal handler in the supervisor.
- [x] Map SIGINT/SIGTERM/SIGQUIT to a graceful shutdown-all, and a second termination signal (or timeout expiry) to a forced abort-and-exit.
- [x] Map SIGUSR1 to "restart all mapper units", leaving the agent running.
- [x] Coalesce restart requests: a restart for a unit already restarting or in backoff is ignored, so repeated SIGUSR1s do not stack.
- [x] Route both the SIGUSR1 path and the (future) control plane through one internal "restart units matching X" action, so the future unix-socket verb is a drop-in second trigger.

## 5. Logging

- [x] Initialise a single tracing subscriber for the whole process in the supervised run mode.
- [x] Attribute each log record to its originating component via a field or span, so the combined stream is unambiguous.

## 6. Tests

- [x] Integration test: a component that crashes is restarted while the other components keep running. (`supervisor.rs::a_crashing_unit_is_restarted_while_others_keep_running`)
- [x] Integration test: repeated crashes trigger backoff and eventually give up without killing the process. (`supervisor.rs::repeated_crashes_back_off_and_eventually_give_up_without_exiting`)
- [x] Integration test: SIGUSR1 restarts the mapper but leaves the agent running. (`supervisor.rs::restart_mappers_restarts_only_mappers`, exercising the `RestartMappers` action the SIGUSR1 handler emits)
- [x] Integration test: SIGTERM performs an orderly shutdown of all components. (`supervisor.rs::shutdown_drains_all_units_and_exits`, exercising the `ShutdownAll` action the SIGTERM handler emits)
