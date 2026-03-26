# Custom Mapper Runtime

## Purpose

Defines the runtime behaviour of user-defined mappers: how they are invoked via the CLI, what services they start (MQTT bridge and/or flows engine), their service identity and naming conventions, and how multiple mapper instances coexist.

## Requirements

### Requirement: CLI invocation via external subcommand
Users SHALL be able to start a user-defined mapper using `tedge-mapper <name>`, where `<name>` is the mapper directory name under `/etc/tedge/mappers/`. This is implemented via clap's `external_subcommand` mechanism, which captures any subcommand name that does not match a built-in variant (`c8y`, `az`, `aws`, etc.).

At startup, the mapper name is validated by `validate_and_load`, which performs these checks in order:
1. Matches `[a-z][a-z0-9-]*` — otherwise hard error
2. Does not start with `bridge-` — otherwise hard error (would collide with the `tedge-mapper-bridge-{name}` bridge sub-service naming pattern)
3. The mapper directory exists — otherwise error listing available mappers
4. If `bridge/` is present, `mapper.toml` must also be present and valid — otherwise error

The mapper SHALL then create the `flows/` subdirectory if it does not already exist, and start in flows-only mode if no bridge is configured.

Extra arguments after the name SHALL be rejected with a clear error.

#### Scenario: Starting a user-defined mapper by name
- **WHEN** a user runs `tedge-mapper thingsboard` and `/etc/tedge/mappers/thingsboard/` exists
- **THEN** `tedge-mapper` SHALL start the mapper, launching whichever of the MQTT bridge and flows engine are applicable given the directory contents

#### Scenario: Empty mapper directory starts in flows-only mode
- **WHEN** a user runs `tedge-mapper mycloud` and `/etc/tedge/mappers/mycloud/` exists but is empty
- **THEN** `tedge-mapper` SHALL start successfully in flows-only mode and SHALL create `/etc/tedge/mappers/mycloud/flows/`

#### Scenario: Mapper directory does not exist
- **WHEN** a user runs `tedge-mapper mycloud` and no directory `/etc/tedge/mappers/mycloud/` exists
- **THEN** `tedge-mapper` SHALL report an error indicating that no mapper configuration was found for `mycloud` and SHALL list available user-defined mappers

#### Scenario: Mapper directory has flows/ but no mapper.toml
- **WHEN** a user runs `tedge-mapper mycloud` and `/etc/tedge/mappers/mycloud/flows/` exists but no `mapper.toml` is present
- **THEN** `tedge-mapper` SHALL start successfully as a flows-only mapper (no cloud MQTT bridge)

#### Scenario: Extra arguments after mapper name rejected
- **WHEN** a user runs `tedge-mapper thingsboard --unknown-flag`
- **THEN** `tedge-mapper` SHALL exit with a clear error — extra arguments after the mapper name are not supported

### Requirement: Custom mapper runs bridge and/or flows
A mapper SHALL start the built-in MQTT bridge and/or the flows engine depending on the contents of its directory. There SHALL be no built-in cloud-specific converter — all message transformation is handled by user-provided flow scripts or bridge rules.

#### Scenario: Mapper starts bridge when mapper.toml is present
- **WHEN** a mapper starts and its `mapper.toml` contains valid connection settings and a resolvable device ID (from the certificate CN or `device.id`)
- **THEN** the mapper SHALL establish an MQTT bridge between the local broker and the configured cloud endpoint

#### Scenario: Bridge fails to start without a resolvable device ID
- **WHEN** a mapper starts with `mapper.toml` present but no device ID can be determined (password auth with no `device.id` set, or certificate CN is empty and `device.id` is absent)
- **THEN** the mapper SHALL fail to start the bridge with a clear error indicating that a device ID is required

#### Scenario: Mapper starts flows engine when flows directory is present
- **WHEN** a mapper starts and its `flows/` directory contains flow scripts
- **THEN** the mapper SHALL load and run the flow scripts, processing messages according to the flow definitions

#### Scenario: Mapper with bridge rules
- **WHEN** a mapper starts and its `bridge/` directory contains bridge rule TOML files
- **THEN** the mapper SHALL load the bridge rules (expanding any `${mapper.*}` and `${config.*}` template variables) and configure the MQTT bridge accordingly

#### Scenario: Mapper without flows directory
- **WHEN** a mapper starts and its directory has no `flows/` subdirectory
- **THEN** the mapper SHALL start successfully with only the MQTT bridge (no flows engine)

#### Scenario: Mapper without mapper.toml
- **WHEN** a mapper starts and its directory has no `mapper.toml`
- **THEN** the mapper SHALL start successfully with only the flows engine (no MQTT bridge to cloud)

#### Scenario: Empty mapper directory starts in flows-only mode
- **WHEN** a mapper starts and its directory has no `mapper.toml`, `bridge/`, or `flows/`
- **THEN** the mapper SHALL start successfully in flows-only mode and SHALL create the `flows/` subdirectory

### Requirement: Startup scan warns about misconfigured mapper directories
On startup, the mapper SHALL scan `/etc/tedge/mappers/` and emit a warning for any subdirectory that contains a `bridge/` subdirectory but no `mapper.toml`. Such a directory will fail to start — bridge rules require connection settings — and is likely a configuration mistake.

All other directories (including empty ones) are considered valid mapper directories and do not generate a warning.

#### Scenario: bridge/ without mapper.toml warns
- **WHEN** `/etc/tedge/mappers/thingsboard/bridge/` exists but no `/etc/tedge/mappers/thingsboard/mapper.toml` is present
- **THEN** the mapper SHALL emit a warning that `thingsboard` has bridge rules but no `mapper.toml` and will fail to start

#### Scenario: Empty directory does not warn
- **WHEN** `/etc/tedge/mappers/thingsboard/` exists but is empty
- **THEN** no warning SHALL be emitted — an empty directory is a valid unstarted mapper

#### Scenario: flows-only directory does not warn
- **WHEN** `/etc/tedge/mappers/thingsboard/flows/` exists with flow scripts but no `mapper.toml`
- **THEN** no warning SHALL be emitted

### Requirement: Service identity follows naming conventions
A mapper's service identity SHALL use `tedge-mapper-{name}` as the service name. This matches the naming convention of built-in mappers (`tedge-mapper-c8y`, `tedge-mapper-az`, `tedge-mapper-aws`) and is not tied to any specific init system.

#### Scenario: Service name for user-defined mapper
- **WHEN** a mapper is started with `tedge-mapper thingsboard`
- **THEN** its service name SHALL be `tedge-mapper-thingsboard`

#### Scenario: Health topic
- **WHEN** a mapper named `thingsboard` starts
- **THEN** it SHALL publish health status on topic `te/device/main/service/tedge-mapper-thingsboard/status/health`

#### Scenario: Lock file
- **WHEN** a mapper named `thingsboard` starts
- **THEN** it SHALL create a lock file at `/run/tedge-mapper-thingsboard.lock` to prevent duplicate instances

#### Scenario: Bridge service name
- **WHEN** a mapper named `thingsboard` starts its built-in MQTT bridge
- **THEN** the bridge service name SHALL be `tedge-mapper-bridge-thingsboard`

### Requirement: Multiple mappers can coexist
Multiple mapper instances SHALL be able to run simultaneously as independent services, each with its own configuration, bridge connection, and flows.

#### Scenario: Two user-defined mappers running concurrently
- **WHEN** mappers named `thingsboard` and `mycloud` are both started
- **THEN** each SHALL run as a separate service with its own MQTT bridge connection, flows engine, health topic, and lock file, without interfering with each other

#### Scenario: User-defined mapper coexists with built-in mapper
- **WHEN** a mapper named `thingsboard` and the built-in `c8y` mapper are both running
- **THEN** both SHALL operate independently — the user-defined mapper does not affect the built-in mapper's configuration, bridge, or behaviour
