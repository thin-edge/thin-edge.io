## 1. Supervisor single-unit construction helper

- [x] 1.1 Add a helper on `Supervisor` (or a free function in `supervisor.rs`) that accepts a service name, `UnitKind`, `RuntimeFactory`, lock, and `LogLevelReloadHandle`, and returns a ready-to-run single-unit `Supervisor`
- [x] 1.2 Extract the supervisor into its own crate (`tedge_supervisor` in `crates/common/`) so `tedge_agent` and `tedge_mapper` can depend on it directly without cyclic dependencies

## 2. Standalone tedge-agent adoption

- [x] 2.1 Replace `log_init()` with `log_init_reloadable_for_services()` in `tedge_agent::run()`, passing `["tedge-agent"]` as the service list
- [x] 2.2 Replace `SignalActor` + `run_to_completion()` with the single-unit supervisor in `tedge_agent::run()`
- [x] 2.3 Remove the `use tedge_signal_ext::SignalActor` import and the `agent.start()` path that wires it

## 3. Standalone tedge-mapper adoption

- [x] 3.1 Replace `log_init()` with `log_init_reloadable_for_services()` in `tedge_mapper::run()`, passing the mapper's service name
- [x] 3.2 Replace `SignalActor` + `run_to_completion()` with the single-unit supervisor in `tedge_mapper::run()`
- [x] 3.3 Remove the `use tedge_signal_ext::SignalActor` import from `tedge_mapper`

## 4. SIGUSR1 handling for standalone

- [x] 4.1 Verify that `restart_mappers()` is a no-op when a standalone agent has no mapper units (no code change expected, just confirm)
- [x] 4.2 Verify that SIGUSR1 on a standalone mapper does not cause a user-visible restart loop or unexpected behaviour

## 5. Testing

- [x] 5.1 Add a unit test: standalone supervisor shuts down cleanly on `Command::ShutdownAll`
- [x] 5.2 Add a unit test: standalone supervisor restarts its unit on crash with backoff
- [x] 5.3 Add a unit test: `Command::ReloadLogLevels` invokes the reload handle
- [x] 5.4 Add a unit test: `Command::RestartMappers` is a no-op for a single agent unit
- [x] 5.5 Manual verification: send SIGHUP to a running standalone `tedge-agent` and confirm log levels reload from `system.toml`
- [x] 5.6 Manual verification: send SIGTERM to a running standalone `tedge-mapper` and confirm graceful shutdown

## 6. Cleanup

- [x] 6.1 Remove `tedge_signal_ext` dependency from `tedge_agent` and `tedge_mapper` Cargo.toml if no other code in those crates uses it
- [x] 6.2 Update module-level doc comments in `supervisor.rs` to reflect that it serves both `tedge run all` and standalone processes
