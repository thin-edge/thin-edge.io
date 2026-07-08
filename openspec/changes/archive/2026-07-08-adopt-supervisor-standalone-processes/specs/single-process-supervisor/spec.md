## MODIFIED Requirements

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
- **THEN** it runs under a single-unit supervisor with SIGHUP log level reloading and crash recovery
- **AND** it acquires its own lock file and MQTT connection
- **AND** it exits through the supervisor's shutdown path rather than calling `process::exit` directly
