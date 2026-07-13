## ADDED Requirements

### Requirement: Standalone processes run under the supervisor

Standalone long-running processes (`tedge-agent`, `tedge-mapper`) SHALL run
under the supervisor with a single unit, providing crash recovery and signal
handling identical to the `tedge run all` path.

#### Scenario: Standalone agent uses the supervisor

- **WHEN** `tedge-agent` is started standalone
- **THEN** it runs under a single-unit supervisor
- **AND** the supervisor owns all signal handling for the process

#### Scenario: Standalone mapper uses the supervisor

- **WHEN** `tedge-mapper <name>` is started standalone
- **THEN** it runs under a single-unit supervisor
- **AND** the supervisor owns all signal handling for the process

#### Scenario: Crash recovery for standalone processes

- **WHEN** a standalone process's component crashes
- **THEN** the supervisor rebuilds and restarts it using the same backoff policy as `tedge run all`
- **AND** the process does not exit unless the restart cap is exceeded

### Requirement: SIGHUP reloads log levels for standalone processes

Standalone long-running processes SHALL support SIGHUP-driven log level
reloading from `system.toml`, using the same reloadable logging infrastructure
as `tedge run all`.

#### Scenario: SIGHUP reloads log levels

- **WHEN** a standalone process receives SIGHUP
- **THEN** log levels are re-read from `system.toml` and applied without restarting the component

#### Scenario: SIGHUP is ignored when log levels are overridden

- **WHEN** a standalone process is started with `--log-level`, `--debug`, or `RUST_LOG`
- **AND** the process receives SIGHUP
- **THEN** the signal is logged as ignored and log levels remain unchanged

### Requirement: Graceful shutdown for standalone processes

Standalone processes SHALL shut down gracefully on SIGINT, SIGTERM, or SIGQUIT,
using the supervisor's shutdown mechanism.

#### Scenario: SIGTERM shuts down a standalone process

- **WHEN** a standalone process receives SIGTERM, SIGINT, or SIGQUIT
- **THEN** the supervisor drains the component within the shutdown timeout and exits

#### Scenario: Second termination signal forces exit

- **WHEN** a standalone process receives a second termination signal during shutdown
- **THEN** the supervisor aborts and exits immediately

### Requirement: SIGUSR1 is not supported for standalone processes

Standalone processes SHALL NOT act on SIGUSR1, as coordinated mapper restart is
not meaningful outside the multi-component `tedge run all` mode.

#### Scenario: SIGUSR1 has no effect on a standalone process

- **WHEN** a standalone process receives SIGUSR1
- **THEN** no component is restarted
- **AND** the signal is ignored

### Requirement: CLI and systemd interface is unchanged

The adoption of the supervisor for standalone processes SHALL NOT change the
command-line interface, systemd service files, or operator-visible behaviour
beyond the addition of SIGHUP support and crash recovery.

#### Scenario: Same binary and arguments

- **WHEN** an operator starts `tedge-agent` or `tedge-mapper <name>` with existing arguments
- **THEN** the process starts and operates as before

#### Scenario: Lock files are still acquired

- **WHEN** a standalone process starts under the supervisor
- **THEN** it acquires its single-instance lock file as before
- **AND** a second instance is prevented from starting
