# single-process-supervisor Specification

## Purpose

Provide a tedge run mode that supervises multiple core components (the agent and a
mapper) within a single process, removing the need for an external init system to
start and keep the components running. The supervisor isolates component crashes,
restarts failed components, owns process signal handling, and attributes logs and
MQTT connections per component.

## Requirements

### Requirement: Single-process run mode

tedge SHALL provide a run mode that supervises multiple core components (the agent
and a mapper) within a single process, so that no external init system is required
to start or keep the components running.

#### Scenario: Starting all components in one process

- **WHEN** the user starts the single-process run mode with the agent and a mapper enabled
- **THEN** both components run inside the one process
- **AND** each component connects to the local MQTT broker as it does when run standalone

#### Scenario: Standalone invocation uses the supervisor

- **WHEN** a component is invoked standalone (e.g. `tedge-mapper c8y` or `tedge-agent`)
- **THEN** it runs under a single-unit supervisor with SIGHUP log level reloading
- **AND** it acquires its own lock file and MQTT connection
- **AND** it exits through the supervisor's shutdown path rather than calling `process::exit` directly
- **AND** a crash of its component exits the process, leaving recovery to the init system

### Requirement: Crash isolation and automatic restart

The supervisor SHALL isolate component failures in the single-process run mode so
that one component crashing does not terminate the others, and SHALL automatically
restart a crashed component.

#### Scenario: One component crashes

- **WHEN** a supervised component exits with an error or panics
- **THEN** the other supervised components keep running
- **AND** the failed component is rebuilt and restarted

#### Scenario: Repeated crashes are bounded

- **WHEN** a component crashes repeatedly within the restart window
- **THEN** the supervisor applies a backoff between restart attempts
- **AND** after the maximum number of attempts it stops restarting that component and logs that it has given up, without exiting the process

### Requirement: Restart requests exit the process

A supervised component that requires a restart (after a self-update, or an update
of its own configuration) SHALL cause the supervisor to drain every component and
exit the process with a non-zero code, since only re-executing the binary makes
such an update effective.

#### Scenario: Self-update exits the single-process run mode

- **WHEN** a supervised component reports that a restart is required
- **THEN** the other components are drained gracefully
- **AND** the process exits with a non-zero code so its service manager can restart it

### Requirement: Best-effort start ordering

The supervisor SHALL start the agent before mappers and stop them in reverse order,
without imposing a readiness dependency between them.

#### Scenario: Agent is spawned before mappers

- **WHEN** the single-process run mode starts
- **THEN** the agent is spawned before any mapper

#### Scenario: Mappers do not wait on agent readiness

- **WHEN** the agent is slow to become ready or is not running
- **THEN** a mapper still starts and operates, preserving any-order startup correctness

### Requirement: Graceful shutdown on termination signals

The supervisor SHALL own process signal handling and perform an orderly shutdown of
all components on a termination signal.

#### Scenario: SIGTERM drains all components

- **WHEN** the process receives SIGTERM, SIGINT, or SIGQUIT
- **THEN** the supervisor requests each component to shut down, waits for them to drain within the shutdown timeout, and then exits

#### Scenario: Second termination signal forces exit

- **WHEN** a second termination signal arrives, or the shutdown timeout expires
- **THEN** the supervisor aborts remaining components and exits immediately

### Requirement: Restart mappers on SIGUSR1

The supervisor SHALL restart all mapper components, and only mapper components, when
it receives SIGUSR1.

#### Scenario: SIGUSR1 restarts mappers only

- **WHEN** the process receives SIGUSR1
- **THEN** every mapper component is restarted
- **AND** the agent continues running uninterrupted

#### Scenario: Restart requests are coalesced

- **WHEN** a restart is requested for a component that is already restarting or in restart backoff
- **THEN** the request is ignored rather than queued or stacked

### Requirement: Per-component MQTT connections

Each supervised component SHALL maintain its own MQTT connection and session so that
restarting a component preserves broker-side offline message queueing.

#### Scenario: Restart replays queued messages

- **WHEN** a component is restarted
- **THEN** its MQTT connection drops and reconnects under the same session
- **AND** messages the broker queued while it was disconnected are delivered on reconnect

### Requirement: Per-component log attribution

The single-process run mode SHALL initialise one logging subscriber and attribute
each log record to the component that produced it.

#### Scenario: Logs identify their component

- **WHEN** multiple components log to the shared process log stream
- **THEN** each record identifies the originating component
