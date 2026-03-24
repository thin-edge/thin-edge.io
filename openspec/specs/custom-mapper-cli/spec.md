# Custom Mapper CLI

## Purpose

Defines the `tedge` CLI subcommands for inspecting, testing, and reading configuration of user-defined mappers registered under `/etc/tedge/mappers/`.

## Requirements

### Requirement: tedge mapper list
`tedge mapper list` SHALL scan `/etc/tedge/mappers/` and print all subdirectories, since any directory under that path is a valid mapper. For each mapper, the output SHALL include the mapper name and its `cloud_type` value if present in `mapper.toml`.

#### Scenario: Listing mappers with and without cloud_type
- **WHEN** `/etc/tedge/mappers/` contains `c8y/` (with `cloud_type = "c8y"`), `thingsboard/` (no `cloud_type`), and `production/` (with `cloud_type = "c8y"`)
- **THEN** `tedge mapper list` SHALL print all three mappers, showing `cloud_type=c8y` for `c8y` and `production` and no annotation for `thingsboard`

#### Scenario: Flows-only mapper is included
- **WHEN** `/etc/tedge/mappers/` contains `thingsboard/flows/` (with flow scripts) but no `thingsboard/mapper.toml`
- **THEN** `tedge mapper list` SHALL include `thingsboard` with no `cloud_type` annotation

#### Scenario: Empty mapper directory is included
- **WHEN** `/etc/tedge/mappers/` contains `thingsboard/` (empty directory, no `mapper.toml` or `flows/`)
- **THEN** `tedge mapper list` SHALL include `thingsboard` with no `cloud_type` annotation

#### Scenario: Empty mappers directory
- **WHEN** `/etc/tedge/mappers/` contains no subdirectories
- **THEN** `tedge mapper list` SHALL print nothing (or an empty list)

### Requirement: tedge mapper config get
`tedge mapper config get <name>.<key>` SHALL read a value from the named mapper's `mapper.toml`. The argument is split on the first `.`: the segment before the first `.` is the mapper name; the remainder is the TOML key path (supporting nested keys with further `.` separators). Output SHALL be the raw string value, matching `tedge config get` behaviour.

#### Scenario: Reading a top-level key
- **WHEN** a user runs `tedge mapper config get thingsboard.url` and `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "mqtt.thingsboard.io:8883"`
- **THEN** the output SHALL be `mqtt.thingsboard.io:8883`

#### Scenario: Reading a nested key
- **WHEN** a user runs `tedge mapper config get thingsboard.device.cert_path` and the mapper's `mapper.toml` contains `[device]` with `cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"`
- **THEN** the output SHALL be `/etc/tedge/device-certs/tedge-certificate.pem`

#### Scenario: Mapper directory does not exist
- **WHEN** a user runs `tedge mapper config get noexist.url` and no `/etc/tedge/mappers/noexist/` directory exists
- **THEN** `tedge mapper config get` SHALL exit with an error indicating the mapper was not found

#### Scenario: mapper.toml absent
- **WHEN** a user runs `tedge mapper config get thingsboard.url` and `/etc/tedge/mappers/thingsboard/` exists but contains no `mapper.toml`
- **THEN** `tedge mapper config get` SHALL exit with an error indicating that `mapper.toml` is absent

#### Scenario: Key not found in mapper.toml
- **WHEN** a user runs `tedge mapper config get thingsboard.nonexistent.key` and the key path does not exist in the mapper's `mapper.toml`
- **THEN** `tedge mapper config get` SHALL exit with an error indicating the key was not found, including the key name in the message

#### Scenario: Argument with no dot is rejected
- **WHEN** a user runs `tedge mapper config get thingsboard` (no `.key` portion)
- **THEN** `tedge mapper config get` SHALL exit with a usage error indicating the argument must be in `<name>.<key>` format

### Requirement: tedge bridge inspect for custom mappers
`tedge bridge inspect <cloud>` SHALL display the bridge rules for any cloud or custom mapper. The `<cloud>` argument accepts built-in cloud names (`c8y`, `aws`, `az`) or a custom mapper name. For custom mappers, the command reads bridge rule TOML files from `/etc/tedge/mappers/<name>/bridge/`, loads `mapper.toml` for `${mapper.*}` template expansion and auth method resolution, and displays the expanded rules grouped by direction (outbound, inbound, bidirectional).

#### Scenario: Inspecting bridge rules for a custom mapper
- **WHEN** a user runs `tedge bridge inspect thingsboard` and `/etc/tedge/mappers/thingsboard/bridge/` contains bridge rule TOML files
- **THEN** the output SHALL display the expanded bridge rules with their directions, local prefixes, and remote prefixes

#### Scenario: Custom mapper directory does not exist
- **WHEN** a user runs `tedge bridge inspect noexist` and no `/etc/tedge/mappers/noexist/` directory exists
- **THEN** the output SHALL indicate that the custom mapper was not found, including the expected path

#### Scenario: Custom mapper without bridge directory
- **WHEN** a user runs `tedge bridge inspect thingsboard` and `/etc/tedge/mappers/thingsboard/` exists but has no `bridge/` subdirectory
- **THEN** the output SHALL indicate that no bridge configuration directory was found

#### Scenario: Template expansion uses mapper.toml values
- **WHEN** a user runs `tedge bridge inspect thingsboard` and the bridge rules reference `${mapper.bridge.topic_prefix}` and `mapper.toml` contains `[bridge] topic_prefix = "tb"`
- **THEN** the template SHALL expand to `tb` in the displayed rules

#### Scenario: Skipped rules shown with --debug
- **WHEN** a user runs `tedge bridge inspect thingsboard --debug` and some bridge rules are disabled by conditions
- **THEN** the output SHALL include the skipped rules with their reasons, matching the built-in cloud behaviour

### Requirement: tedge bridge test for custom mappers
`tedge bridge test <cloud> <topic>` SHALL test where a specific MQTT topic would be forwarded. The `<cloud>` argument accepts built-in cloud names (`c8y`, `aws`, `az`) or a custom mapper name. For custom mappers, the command loads bridge rules from `/etc/tedge/mappers/<name>/bridge/`, expands templates using `mapper.toml`, and matches the provided topic against all rules to show the forwarding result.

#### Scenario: Topic matches an outbound rule
- **WHEN** a user runs `tedge bridge test thingsboard te/telemetry` and the custom mapper has an outbound rule mapping `te/` to `tb/`
- **THEN** the output SHALL show `te/telemetry` → `tb/telemetry` (outbound)

#### Scenario: Topic does not match any rule
- **WHEN** a user runs `tedge bridge test thingsboard unrelated/topic` and no bridge rule matches
- **THEN** the output SHALL indicate that no matching bridge rule was found

#### Scenario: Custom mapper not found
- **WHEN** a user runs `tedge bridge test noexist some/topic` and no `/etc/tedge/mappers/noexist/` exists
- **THEN** the output SHALL indicate that the custom mapper was not found

#### Scenario: Wildcard topics are rejected
- **WHEN** a user runs `tedge bridge test thingsboard te/#`
- **THEN** the command SHALL exit with an error indicating that wildcard characters are not supported

#### Scenario: Missing topic argument
- **WHEN** a user runs `tedge bridge test thingsboard` (no topic provided)
- **THEN** the command SHALL exit with a clap error indicating that the topic argument is missing
