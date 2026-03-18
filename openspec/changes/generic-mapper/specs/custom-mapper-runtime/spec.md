> **Superseded in part**: the CLI invocation (previously `tedge-mapper custom --profile <name>`), service identity naming (previously `tedge-mapper-custom@{name}`), and directory warning logic have been revised by [`mapper-first-class`](../../mapper-first-class/specs). The updated CLI is `tedge-mapper <name>`, service names are `tedge-mapper-{name}` and `tedge-mapper-bridge-{name}`, and directories without a `mapper.toml` are flagged as unrecognised. Requirements 2, 3, and 4 below remain valid. Requirements 1 and 5 are superseded.

## ADDED Requirements

### Requirement: CLI invocation
> **Superseded by `mapper-first-class`**. The implementation uses `tedge-mapper <name>` (an external clap subcommand) rather than `tedge-mapper custom --profile <name>`.

Users SHALL be able to start a mapper by name using `tedge-mapper <name>`, where `<name>` matches the directory under `/etc/tedge/mappers/`.

#### Scenario: Starting a mapper by name
- **WHEN** a user runs `tedge-mapper thingsboard` and `/etc/tedge/mappers/thingsboard/` exists
- **THEN** `tedge-mapper` SHALL start the mapper, launching whichever of the MQTT bridge and flows engine are applicable given the directory contents

#### Scenario: Mapper does not exist
- **WHEN** a user runs `tedge-mapper mycloud` and no directory `/etc/tedge/mappers/mycloud/` exists
- **THEN** `tedge-mapper` SHALL report an error indicating that no mapper configuration was found for `mycloud`

### Requirement: Custom mapper runs bridge and/or flows
A custom mapper SHALL start the built-in MQTT bridge and/or the flows engine depending on the contents of its directory. There SHALL be no built-in cloud-specific converter — all message transformation is handled by user-provided flow scripts.

#### Scenario: Custom mapper starts bridge when mapper.toml is present
- **WHEN** a custom mapper starts and its `mapper.toml` contains valid connection settings (URL, port, TLS certificates)
- **THEN** the mapper SHALL establish an MQTT bridge between the local broker and the configured cloud endpoint

#### Scenario: Custom mapper starts flows engine when flows directory is present
- **WHEN** a custom mapper starts and its `flows/` directory contains flow scripts
- **THEN** the mapper SHALL load and run the flow scripts, processing messages according to the flow definitions

#### Scenario: Custom mapper with bridge rules
- **WHEN** a custom mapper starts and its `bridge/` directory contains bridge rule TOML files
- **THEN** the mapper SHALL load the bridge rules (expanding any `${mapper.*}` and `${config.*}` template variables) and configure the MQTT bridge accordingly

#### Scenario: Custom mapper without flows directory
- **WHEN** a custom mapper starts and its directory has no `flows/` subdirectory
- **THEN** the mapper SHALL start successfully with only the MQTT bridge (no flows engine)

#### Scenario: Custom mapper without mapper.toml
- **WHEN** a custom mapper starts and its directory has no `mapper.toml`
- **THEN** the mapper SHALL start successfully with only the flows engine (no MQTT bridge to cloud)

#### Scenario: Empty custom mapper directory
- **WHEN** a custom mapper starts and its directory has no `mapper.toml`, `bridge/`, or `flows/`
- **THEN** the mapper SHALL exit with an error indicating that neither connection settings nor flow scripts are present, and the mapper would do nothing

### Requirement: Service identity follows naming conventions
> **Superseded by `mapper-first-class`**. Service names use the mapper name directly, not a `custom@` prefix.

A mapper's service identity SHALL use `tedge-mapper@{name}` as the service name, and `tedge-mapper-bridge-{name}` for the bridge service.

#### Scenario: Service name
- **WHEN** a mapper named `thingsboard` is started with `tedge-mapper thingsboard`
- **THEN** its service name SHALL be `tedge-mapper@thingsboard`

#### Scenario: Health topic
- **WHEN** a mapper named `thingsboard` starts
- **THEN** it SHALL publish health status on topic `te/device/main/service/tedge-mapper@thingsboard/status/health`

#### Scenario: Lock file
- **WHEN** a mapper named `thingsboard` starts
- **THEN** it SHALL create a lock file at `/run/tedge-mapper@thingsboard.lock` to prevent duplicate instances

#### Scenario: Bridge service name
- **WHEN** a mapper named `thingsboard` starts its built-in MQTT bridge
- **THEN** the bridge service name SHALL be `tedge-mapper-bridge-thingsboard`

### Requirement: Multiple custom mappers can coexist
Multiple custom mapper profiles SHALL be able to run simultaneously as independent services, each with its own configuration, bridge connection, and flows.

#### Scenario: Two custom mapper profiles running concurrently
- **WHEN** custom mappers with profiles `thingsboard` and `mycloud` are both started
- **THEN** each SHALL run as a separate service with its own MQTT bridge connection, flows engine, health topic, and lock file, without interfering with each other

#### Scenario: Custom mapper coexists with built-in mapper
- **WHEN** a custom mapper with profile `thingsboard` and the built-in `c8y` mapper are both running
- **THEN** both SHALL operate independently — the custom mapper does not affect the built-in mapper's configuration, bridge, or behaviour

### Requirement: Warn about unrecognised mapper directories
> **Superseded by `mapper-first-class`**. The detection approach changed: any directory that lacks a `mapper.toml` (and is not a known built-in mapper name or its profile variant) is flagged as unrecognised.

On mapper startup, `tedge-mapper` SHALL scan `/etc/tedge/mappers/` and emit a warning for any directory that is not a known built-in mapper name (or its profile variant) and does not contain a `mapper.toml`.

#### Scenario: Directory with mapper.toml is not warned about
- **WHEN** `/etc/tedge/mappers/` contains `thingsboard/` with a `mapper.toml` file
- **THEN** `tedge-mapper` SHALL NOT emit a warning for `thingsboard/`

#### Scenario: Directory without mapper.toml is warned about
- **WHEN** `/etc/tedge/mappers/` contains a directory `oldcloud/` that has no `mapper.toml` and is not a built-in mapper name
- **THEN** `tedge-mapper` SHALL emit a warning about the unrecognised directory

#### Scenario: Built-in mapper directories are not warned about
- **WHEN** `/etc/tedge/mappers/` contains `c8y/`, `c8y.staging/`, and `aws/` (all without `mapper.toml`)
- **THEN** `tedge-mapper` SHALL NOT emit any directory warnings for those directories
