## ADDED Requirements

### Requirement: CLI invocation via custom subcommand
Users SHALL be able to start a custom mapper using `tedge-mapper custom --profile <name>`, where `<name>` is the custom mapper profile name. The unprofiled form `tedge-mapper custom` (using the `custom/` directory) SHALL also be supported. The `custom` subcommand SHALL be a clap-defined subcommand with standard argument parsing.

#### Scenario: Starting a custom mapper by profile
- **WHEN** a user runs `tedge-mapper custom --profile thingsboard` and `/etc/tedge/mappers/custom.thingsboard/` exists
- **THEN** `tedge-mapper` SHALL start the custom mapper, launching whichever of the MQTT bridge and flows engine are applicable given the directory contents

#### Scenario: Starting the unprofiled custom mapper
- **WHEN** a user runs `tedge-mapper custom` and `/etc/tedge/mappers/custom/` exists
- **THEN** `tedge-mapper` SHALL start the custom mapper using that directory

#### Scenario: Custom mapper profile does not exist
- **WHEN** a user runs `tedge-mapper custom --profile mycloud` and no directory `/etc/tedge/mappers/custom.mycloud/` exists
- **THEN** `tedge-mapper` SHALL report an error indicating that no custom mapper configuration was found for profile `mycloud`

### Requirement: Custom mapper runs bridge and/or flows
A custom mapper SHALL start the built-in MQTT bridge and/or the flows engine depending on the contents of its directory. There SHALL be no built-in cloud-specific converter — all message transformation is handled by user-provided flow scripts.

#### Scenario: Custom mapper starts bridge when tedge.toml is present
- **WHEN** a custom mapper starts and its `tedge.toml` contains valid connection settings (URL, port, TLS certificates)
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

#### Scenario: Custom mapper without tedge.toml
- **WHEN** a custom mapper starts and its directory has no `tedge.toml`
- **THEN** the mapper SHALL start successfully with only the flows engine (no MQTT bridge to cloud)

#### Scenario: Empty custom mapper directory
- **WHEN** a custom mapper starts and its directory has no `tedge.toml`, `bridge/`, or `flows/`
- **THEN** the mapper SHALL exit with an error indicating that neither connection settings nor flow scripts are present, and the mapper would do nothing

### Requirement: Service identity follows naming conventions
A custom mapper's service identity SHALL follow the same naming pattern as built-in profiled mappers, using `custom` as the mapper type and the profile name (if any) appended with `@`.

#### Scenario: Service name with profile
- **WHEN** a custom mapper is started with `--profile thingsboard`
- **THEN** its service name SHALL be `tedge-mapper-custom@thingsboard`

#### Scenario: Service name without profile
- **WHEN** a custom mapper is started without `--profile`
- **THEN** its service name SHALL be `tedge-mapper-custom`

#### Scenario: Health topic
- **WHEN** a custom mapper with profile `thingsboard` starts
- **THEN** it SHALL publish health status on topic `te/device/main/service/tedge-mapper-custom@thingsboard/status/health`

#### Scenario: Lock file
- **WHEN** a custom mapper with profile `thingsboard` starts
- **THEN** it SHALL create a lock file at `/run/tedge-mapper-custom@thingsboard.lock` to prevent duplicate instances

#### Scenario: Bridge service name
- **WHEN** a custom mapper with profile `thingsboard` starts its built-in MQTT bridge
- **THEN** the bridge service name SHALL be `tedge-mapper-bridge-custom@thingsboard`

### Requirement: Multiple custom mappers can coexist
Multiple custom mapper profiles SHALL be able to run simultaneously as independent services, each with its own configuration, bridge connection, and flows.

#### Scenario: Two custom mapper profiles running concurrently
- **WHEN** custom mappers with profiles `thingsboard` and `mycloud` are both started
- **THEN** each SHALL run as a separate service with its own MQTT bridge connection, flows engine, health topic, and lock file, without interfering with each other

#### Scenario: Custom mapper coexists with built-in mapper
- **WHEN** a custom mapper with profile `thingsboard` and the built-in `c8y` mapper are both running
- **THEN** both SHALL operate independently — the custom mapper does not affect the built-in mapper's configuration, bridge, or behaviour

### Requirement: Warn about unrecognised mapper directories
On mapper startup, `tedge-mapper` SHALL scan `/etc/tedge/mappers/` and emit a warning for any directory that is not a known built-in mapper name, a profile directory of a known mapper, `custom`, or a `custom.{name}` directory.

#### Scenario: Unrecognised directory with typo
- **WHEN** `/etc/tedge/mappers/` contains a directory named `custom.thingboard` (missing an 's') that was intended as a custom mapper profile but has never been started
- **THEN** `tedge-mapper` SHALL emit a warning indicating that `custom.thingboard` is a recognised custom mapper profile directory (though a warning may be appropriate for directories that appear misconfigured)

#### Scenario: Unrecognised non-custom directory
- **WHEN** `/etc/tedge/mappers/` contains a directory named `thingsboard` (no `custom.` prefix) that does not match any built-in mapper name
- **THEN** `tedge-mapper` SHALL emit a warning indicating that `thingsboard` is not recognised as a built-in mapper, custom mapper profile, or profile directory

#### Scenario: All directories are recognised
- **WHEN** `/etc/tedge/mappers/` contains only `c8y/`, `c8y.staging/`, `custom.thingsboard/`, and `aws/`
- **THEN** `tedge-mapper` SHALL NOT emit any directory warnings

#### Scenario: Stale directory from removed mapper
- **WHEN** `/etc/tedge/mappers/` contains a directory `oldcloud/` that is not a built-in mapper and does not match `custom` or `custom.{name}`
- **THEN** `tedge-mapper` SHALL emit a warning about the unrecognised directory
