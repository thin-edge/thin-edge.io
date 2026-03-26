# Custom Mapper CLI

## Purpose

Defines the `tedge` CLI subcommands for inspecting, testing, and reading configuration of user-defined mappers registered under `/etc/tedge/mappers/`.

## Requirements

### Requirement: tedge mapper list
`tedge mapper list` SHALL scan `/etc/tedge/mappers/` and print all subdirectories. For each mapper, the output SHALL include the mapper name, `cloud_type` (if present in `mapper.toml`), `url` (if present in `mapper.toml`), and the effective device identity. The effective device identity is determined using the same resolution chain as the mapper runtime: certificate CN (for cert or auto auth) → explicit `device.id` in `mapper.toml` → root `tedge.toml` device_id. Each displayed identity SHALL include a bracketed tag indicating its source (e.g., `[cert CN]`, `[mapper.toml]`, `[tedge.toml]`). If the device identity cannot be determined (e.g., cert auth is in use but the cert file cannot be read), the identity column SHALL be left blank for that mapper. The command SHALL NOT fail due to a single mapper's configuration being unreadable.

The `cloud_type` field is a valid optional metadata field in `mapper.toml` that names the cloud integration type (e.g., `"c8y"`). Its presence SHALL NOT cause `load_mapper_config` to error. An unrecognised `cloud_type` value SHALL be rejected at parse time.

#### Scenario: Mapper with url and cert-auth device identity
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "mqtt.thingsboard.io:8883"` and a readable `device.cert_path` whose CN is `my-device-001`
- **THEN** the output for `thingsboard` SHALL include `mqtt.thingsboard.io:8883` and `my-device-001 [cert CN]`

#### Scenario: Mapper with device identity from tedge.toml
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains no `device.cert_path` or `device.id`, and the root `tedge.toml` has `device_id = "root-device"`
- **THEN** the output for `thingsboard` SHALL include `root-device [tedge.toml]`

#### Scenario: Mapper with unreadable certificate
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` specifies cert auth with a `device.cert_path` that cannot be read
- **THEN** the identity column for `thingsboard` SHALL be blank and `tedge mapper list` SHALL still complete successfully and list all other mappers

#### Scenario: Flows-only mapper has blank url and identity
- **WHEN** `/etc/tedge/mappers/thingsboard/` exists with no `mapper.toml`
- **THEN** the url and identity columns for `thingsboard` SHALL be blank

#### Scenario: Listing mappers with and without cloud_type
- **WHEN** `/etc/tedge/mappers/` contains `c8y/` (with `cloud_type = "c8y"`), `thingsboard/` (no `cloud_type`), and `production/` (with `cloud_type = "c8y"`)
- **THEN** `tedge mapper list` SHALL print all three mappers, showing `cloud_type=c8y` for `c8y` and `production` and no cloud_type for `thingsboard`

#### Scenario: Empty mappers directory
- **WHEN** `/etc/tedge/mappers/` contains no subdirectories
- **THEN** `tedge mapper list` SHALL print a message to stderr indicating no mappers were found

### Requirement: tedge mapper config get
`tedge mapper config get <name>.<key>` SHALL return the effective resolved value for the named key, not the raw value from `mapper.toml`. For schema-level keys (`device.id`, `device.cert_path`, `device.key_path`, `device.root_cert_path`), the effective value is determined using the same resolution chain as the mapper runtime, including cert/key fallback to root `tedge.toml` and certificate CN inference for `device.id`. For all other keys, the raw TOML value from `mapper.toml` is returned. The resolved value SHALL be printed on stdout. A human-readable annotation explaining the source of the value (e.g., which file it was read from, or that it was inferred from a certificate CN) SHALL be written to stderr. If the effective value cannot be determined (e.g., cert auth is in use but the cert is unreadable), the command SHALL exit with an error.

#### Scenario: device.id inferred from certificate CN
- **WHEN** a user runs `tedge mapper config get thingsboard.device.id` and cert auth is in use with a readable cert whose CN is `my-device-001`
- **THEN** stdout SHALL contain `my-device-001` and stderr SHALL contain an annotation referencing the certificate file path

#### Scenario: device.cert_path inherited from tedge.toml
- **WHEN** a user runs `tedge mapper config get thingsboard.device.cert_path` and `mapper.toml` contains no `device.cert_path`, but root `tedge.toml` has `device.cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"`
- **THEN** stdout SHALL contain `/etc/tedge/device-certs/tedge-certificate.pem` and stderr SHALL contain an annotation indicating the value was inherited from `tedge.toml`

#### Scenario: Relative device.cert_path resolved to absolute
- **WHEN** a user runs `tedge mapper config get thingsboard.device.cert_path` and `mapper.toml` contains `cert_path = "cert.pem"` under `[device]`
- **THEN** stdout SHALL contain the absolute path (e.g., `/etc/tedge/mappers/thingsboard/cert.pem`) and stderr SHALL contain an annotation indicating the original relative path

#### Scenario: Custom key returns raw TOML value
- **WHEN** a user runs `tedge mapper config get thingsboard.bridge.topic_prefix` and `mapper.toml` contains `[bridge]` with `topic_prefix = "tb"`
- **THEN** stdout SHALL contain `tb` and stderr SHALL contain an annotation indicating the value was read from `mapper.toml`

#### Scenario: device.id unavailable due to unreadable cert
- **WHEN** a user runs `tedge mapper config get thingsboard.device.id` and cert auth is in use but the cert file cannot be read
- **THEN** the command SHALL exit with an error indicating that the device identity could not be determined

#### Scenario: Mapper directory does not exist
- **WHEN** a user runs `tedge mapper config get noexist.url` and no `/etc/tedge/mappers/noexist/` directory exists
- **THEN** `tedge mapper config get` SHALL exit with an error indicating the mapper was not found

#### Scenario: mapper.toml absent
- **WHEN** a user runs `tedge mapper config get thingsboard.url` and `/etc/tedge/mappers/thingsboard/` exists but contains no `mapper.toml`
- **THEN** `tedge mapper config get` SHALL exit with an error indicating that `mapper.toml` is absent

#### Scenario: Key not found
- **WHEN** a user runs `tedge mapper config get thingsboard.nonexistent.key` and the key path does not exist in `mapper.toml` and is not a schema-level key with a resolvable fallback
- **THEN** `tedge mapper config get` SHALL exit with an error indicating the key was not found

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
