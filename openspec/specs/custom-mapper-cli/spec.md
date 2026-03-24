# Custom Mapper CLI

## Purpose

Defines the `tedge mapper` subcommands for inspecting and reading configuration of user-defined mappers registered under `/etc/tedge/mappers/`.

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
