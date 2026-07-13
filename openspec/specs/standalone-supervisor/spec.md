# standalone-supervisor Specification

## Purpose

Provide standalone long-running processes (`tedge-agent`, `tedge-mapper`) with the
same supervisor infrastructure used by `tedge run all`, giving them SIGHUP log level
reloading and graceful shutdown without changing the CLI or systemd interface.
Crash recovery stays with the init system: any unit failure exits the process.

## Requirements

### Requirement: Standalone processes run under the supervisor

Standalone long-running processes (`tedge-agent`, `tedge-mapper`) SHALL run
under the supervisor with a single unit, providing signal handling identical to
the `tedge run all` path while leaving crash recovery to the init system.

#### Scenario: Standalone agent uses the supervisor

- **WHEN** `tedge-agent` is started standalone
- **THEN** it runs under a single-unit supervisor
- **AND** the supervisor owns all signal handling for the process

#### Scenario: Standalone mapper uses the supervisor

- **WHEN** `tedge-mapper <name>` is started standalone
- **THEN** it runs under a single-unit supervisor
- **AND** the supervisor owns all signal handling for the process

#### Scenario: Crash of a standalone process exits the process

- **WHEN** a standalone process's component crashes or fails to build
- **THEN** the process exits with a non-zero code
- **AND** the init system applies its restart policy, exactly as it did before the supervisor was adopted

#### Scenario: Clean exit of a standalone process ends the process

- **WHEN** a standalone process's component finishes without error
- **THEN** the process exits with a zero code, exactly as it did before the supervisor was adopted

### Requirement: Restart requests exit the process

A component that requires a restart (after a self-update, or an update of its own
configuration) SHALL cause the process to exit with a non-zero code, so the init
system re-executes the binary.

#### Scenario: Self-update restarts the process

- **WHEN** the component reports that a restart is required
- **THEN** the process exits with a non-zero code
- **AND** the init system restarts it, running the updated binary and configuration

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
beyond the addition of SIGHUP support.

#### Scenario: Same binary and arguments

- **WHEN** an operator starts `tedge-agent` or `tedge-mapper <name>` with existing arguments
- **THEN** the process starts and operates as before

#### Scenario: Standalone log format is unchanged

- **WHEN** a standalone process emits log records
- **THEN** the records match the format used before the supervisor was adopted, without a component attribution prefix

#### Scenario: Configured log levels apply process-wide

- **WHEN** a standalone process is started with a log level configured for its service in `system.toml`
- **THEN** every record the process emits is filtered at that level, without relying on component span attribution

#### Scenario: Lock files are still acquired

- **WHEN** a standalone process starts under the supervisor
- **THEN** it acquires its single-instance lock file as before
- **AND** a second instance is prevented from starting
