> **Superseded in part**: directory naming (previously `custom.{name}/`) and some directory layout scenarios have been revised by [`mapper-first-class`](../../mapper-first-class/specs). Mappers now live at `{name}/` directly under `/etc/tedge/mappers/`. Requirements 1–3 below remain valid except where noted. Requirement 4 (`${mapper.*}` namespace) is unchanged.

## ADDED Requirements

### Requirement: Custom mapper directory layout
A custom mapper SHALL be defined by a directory under `/etc/tedge/mappers/` named after the mapper (e.g. `thingsboard/`). The directory MAY contain any combination of: a `mapper.toml` configuration file, a `bridge/` subdirectory with bridge rule TOML files, and a `flows/` subdirectory with flow scripts.

#### Scenario: Minimal custom mapper — flows only
- **WHEN** a user creates `/etc/tedge/mappers/thingsboard/flows/telemetry.js`
- **THEN** `tedge-mapper` SHALL recognise `thingsboard` as a valid mapper and start the flows engine

#### Scenario: Full custom mapper with bridge and flows
- **WHEN** a user creates `/etc/tedge/mappers/thingsboard/` containing `mapper.toml`, `bridge/rules.toml`, and `flows/telemetry.js`
- **THEN** the custom mapper SHALL load the configuration, bridge rules, and flow scripts from their respective locations

#### Scenario: Empty custom mapper directory
- **WHEN** a user creates `/etc/tedge/mappers/thingsboard/` with no `bridge/` or `flows/` subdirectory
- **THEN** `tedge-mapper` SHALL report an error indicating that neither connection settings (`mapper.toml` with `bridge/`) nor flow scripts (`flows/`) are present

#### Scenario: Bridge rules without mapper.toml
- **WHEN** a user creates `/etc/tedge/mappers/thingsboard/bridge/rules.toml` but no `mapper.toml`
- **THEN** `tedge-mapper` SHALL report an error indicating that bridge rules require connection settings (`mapper.toml`)

### Requirement: Custom mapper configuration file
When present, the custom mapper's `mapper.toml` SHALL contain connection and device identity settings needed to establish the MQTT bridge to the cloud. The configuration file is parsed directly by the custom mapper and is not part of the global `tedge_config` schema. The `mapper.toml` is OPTIONAL — it is only required when the mapper establishes a cloud connection via the built-in MQTT bridge.

#### Scenario: Configuration with connection details
- **WHEN** a custom mapper's `mapper.toml` contains a top-level `url` field in `{host}:{port}` format (using the `HostPort` type; port is optional), and a `[device]` section with `cert_path` and `key_path` fields
- **THEN** the custom mapper SHALL use these values to configure the MQTT bridge connection

#### Scenario: Configuration with additional custom fields
- **WHEN** a custom mapper's `mapper.toml` contains additional TOML keys beyond the required connection and device settings (e.g. `[bridge]` with `topic_prefix`)
- **THEN** the custom mapper SHALL make all fields available via the `${mapper.*}` template namespace in bridge rule templates

#### Scenario: Invalid TOML in configuration file
- **WHEN** a custom mapper's `mapper.toml` contains invalid TOML syntax
- **THEN** `tedge-mapper` SHALL report a parse error with the file path and error location

### Requirement: Custom mapper config is separate from global tedge_config
Custom mapper configuration SHALL NOT be part of the `define_tedge_config!` macro or the global `tedge_config` schema. Users SHALL configure custom mappers by editing the mapper's `mapper.toml` directly, not via `tedge config set/get`.

#### Scenario: tedge config does not see custom mapper settings
- **WHEN** a user runs `tedge config list`
- **THEN** the output SHALL NOT include any settings from custom mapper `mapper.toml` files

#### Scenario: User edits custom mapper config directly
- **WHEN** a user edits `/etc/tedge/mappers/thingsboard/mapper.toml` to change `url`
- **THEN** the change takes effect the next time the custom mapper is started (no `tedge config set` needed)

### Requirement: Bridge templates support mapper config namespace
The bridge template system SHALL support a `${mapper.*}` variable namespace that resolves against the custom mapper's own `mapper.toml`. This namespace is available in **string template fields** (`local_prefix`, `remote_prefix`, and `topic`) within bridge rule TOML files located in the mapper's `bridge/` directory. It is only populated when a `mapper.toml` is present.

The `${mapper.*}` namespace is NOT supported in:
- `if =` condition expressions — these only accept `${config.*}` boolean references or `${connection.auth_method} == '...'`
- `for =` loop source expressions — these only accept `${config.*}` template set references or literal TOML arrays

#### Scenario: Referencing a mapper config value in a bridge template
- **WHEN** a bridge rule template contains `${mapper.bridge.topic_prefix}` and the mapper's `mapper.toml` contains `[bridge]` with `topic_prefix = "tb"`
- **THEN** the template SHALL expand to `tb`

#### Scenario: Referencing a top-level mapper config value
- **WHEN** a bridge rule template contains `${mapper.url}` and the mapper's `mapper.toml` contains a top-level `url = "mqtt.thingsboard.io:8883"`
- **THEN** the template SHALL expand to `mqtt.thingsboard.io:8883`

#### Scenario: Referencing a nested mapper config value in a topic template
- **WHEN** a bridge rule `topic` contains `${mapper.prefix}/${item}` and the mapper's `mapper.toml` contains `prefix = "tb"`, and the loop iterates over `"telemetry"`
- **THEN** the topic SHALL expand to `tb/telemetry`

#### Scenario: Referencing a non-existent mapper config key
- **WHEN** a bridge rule template contains `${mapper.nonexistent.key}` and no such key exists in the mapper's `mapper.toml`
- **THEN** the template system SHALL report an error indicating the key was not found, and SHALL include the key name in the error message

#### Scenario: Combining mapper and global config references
- **WHEN** a bridge rule template contains both `${mapper.bridge.topic_prefix}` and `${config.mqtt.port}`
- **THEN** both variables SHALL resolve correctly — `${mapper.*}` from the mapper's `mapper.toml` and `${config.*}` from the global thin-edge config

#### Scenario: `${mapper.*}` without a mapper config present
- **WHEN** a bridge rule template contains `${mapper.some.key}` but no `mapper.toml` is present for the mapper
- **THEN** the template system SHALL report an error indicating that `${mapper.*}` is only valid in custom mapper bridge rules

#### Scenario: `${mapper.*}` rejected in `if` conditions
- **WHEN** a bridge rule file contains `if = "${mapper.some_flag}"`
- **THEN** the template system SHALL report a clear parse error — `${mapper.*}` is not a valid condition expression; only `${config.*}` boolean references and `${connection.auth_method} == '...'` are accepted

#### Scenario: `${mapper.*}` rejected in `for` loop sources
- **WHEN** a bridge rule file contains a `[[template_rule]]` with `for = "${mapper.some_list}"`
- **THEN** the template system SHALL report a clear parse error — `${mapper.*}` is not a valid loop source; only `${config.*}` template set references and literal TOML arrays are accepted

#### Out of scope: Built-in mappers and the mapper namespace
`${mapper.*}` is only populated for custom mappers, whose config is already a raw TOML table. Built-in mappers (c8y, az, aws) use typed Rust config structs; serialising those to a TOML table to back `${mapper.*}` is non-trivial and unnecessary since `${config.c8y.*}` (etc.) already provides access to all built-in mapper config values. Supporting `${mapper.*}` in built-in bridge rules is deferred to a future change if demand warrants it.
