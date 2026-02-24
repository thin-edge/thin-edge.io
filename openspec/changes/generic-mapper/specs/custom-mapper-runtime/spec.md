## ADDED Requirements

### Requirement: CLI invocation via custom subcommand
Users SHALL be able to start a custom mapper using `tedge-mapper custom --profile <name>`, where `<name>` is the `_`-prefixed custom mapper name. The `custom` subcommand SHALL be a clap-defined subcommand with standard argument parsing.

#### Scenario: Starting a custom mapper
- **WHEN** a user runs `tedge-mapper custom --profile _thingsboard` and `/etc/tedge/mappers/_thingsboard/tedge.toml` exists
- **THEN** `tedge-mapper` SHALL start the custom mapper, launching the MQTT bridge and flows engine

#### Scenario: Custom mapper directory does not exist
- **WHEN** a user runs `tedge-mapper custom --profile _mycloud` and no directory `/etc/tedge/mappers/_mycloud/` exists
- **THEN** `tedge-mapper` SHALL report an error indicating that no custom mapper configuration was found for `_mycloud`

#### Scenario: Custom mapper name missing underscore prefix
- **WHEN** a user runs `tedge-mapper custom --profile thingsboard` (no `_` prefix)
- **THEN** `tedge-mapper` SHALL report an error indicating that custom mapper names must start with `_`

### Requirement: Custom mapper runs bridge and flows
A custom mapper SHALL start the built-in MQTT bridge and the flows engine as a single service, using the configuration, bridge rules, and flow scripts from the mapper's directory. There SHALL be no built-in cloud-specific converter — all message transformation is handled by user-provided flow scripts.

#### Scenario: Custom mapper starts bridge with cloud connection
- **WHEN** a custom mapper starts and its `tedge.toml` contains valid connection settings (URL, port, TLS certificates)
- **THEN** the mapper SHALL establish an MQTT bridge between the local broker and the configured cloud endpoint

#### Scenario: Custom mapper starts flows engine
- **WHEN** a custom mapper starts and its `flows/` directory contains flow scripts
- **THEN** the mapper SHALL load and run the flow scripts, processing messages according to the flow definitions

#### Scenario: Custom mapper with bridge rules
- **WHEN** a custom mapper starts and its `bridge/` directory contains bridge rule TOML files
- **THEN** the mapper SHALL load the bridge rules (expanding any `${mapper.*}` and `${config.*}` template variables) and configure the MQTT bridge accordingly

#### Scenario: Custom mapper without flows directory
- **WHEN** a custom mapper starts and its directory has no `flows/` subdirectory
- **THEN** the mapper SHALL start successfully with only the MQTT bridge (no flows engine)

#### Scenario: Custom mapper without bridge directory
- **WHEN** a custom mapper starts and its directory has no `bridge/` subdirectory
- **THEN** the mapper SHALL start successfully with only the flows engine (no MQTT bridge to cloud)

### Requirement: Service identity follows naming conventions
A custom mapper's service identity SHALL follow the same naming pattern as built-in mappers, using the custom mapper name (including the `_` prefix) in place of the built-in mapper name.

#### Scenario: Service name
- **WHEN** a custom mapper named `_thingsboard` starts
- **THEN** its service name SHALL be `tedge-mapper-_thingsboard`

#### Scenario: Health topic
- **WHEN** a custom mapper named `_thingsboard` starts
- **THEN** it SHALL publish health status on topic `te/device/main/service/tedge-mapper-_thingsboard/status/health`

#### Scenario: Lock file
- **WHEN** a custom mapper named `_thingsboard` starts
- **THEN** it SHALL create a lock file at `/run/tedge-mapper-_thingsboard.lock` to prevent duplicate instances

#### Scenario: Bridge service name
- **WHEN** a custom mapper named `_thingsboard` starts its built-in MQTT bridge
- **THEN** the bridge service name SHALL be `tedge-mapper-bridge-_thingsboard`

### Requirement: Multiple custom mappers can coexist
Multiple custom mappers SHALL be able to run simultaneously as independent services, each with its own configuration, bridge connection, and flows.

#### Scenario: Two custom mappers running concurrently
- **WHEN** custom mappers `_thingsboard` and `_mycloud` are both started
- **THEN** each SHALL run as a separate service with its own MQTT bridge connection, flows engine, health topic, and lock file, without interfering with each other

#### Scenario: Custom mapper coexists with built-in mapper
- **WHEN** a custom mapper `_thingsboard` and the built-in `c8y` mapper are both running
- **THEN** both SHALL operate independently — the custom mapper does not affect the built-in mapper's configuration, bridge, or behaviour

### Requirement: Warn about unrecognised mapper directories
On mapper startup, `tedge-mapper` SHALL scan `/etc/tedge/mappers/` and emit a warning for any directory that is not a known built-in mapper name, a profile directory of a known mapper, or a custom mapper directory (starting with `_`).

#### Scenario: Unrecognised directory with typo
- **WHEN** `/etc/tedge/mappers/` contains a directory named `thingboard` (missing an 's') that does not match any built-in mapper name
- **THEN** `tedge-mapper` SHALL emit a warning indicating that `thingboard` is not recognised as a built-in mapper, custom mapper, or profile directory

#### Scenario: All directories are recognised
- **WHEN** `/etc/tedge/mappers/` contains only `c8y/`, `c8y.staging/`, `_thingsboard/`, and `aws/`
- **THEN** `tedge-mapper` SHALL NOT emit any directory warnings

#### Scenario: Stale directory from removed mapper
- **WHEN** `/etc/tedge/mappers/` contains a directory `oldcloud/` that is not a built-in mapper and does not start with `_`
- **THEN** `tedge-mapper` SHALL emit a warning about the unrecognised directory
