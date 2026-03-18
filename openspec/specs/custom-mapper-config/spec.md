# Custom Mapper Configuration

## Purpose

Defines how custom mappers are configured on disk under `/etc/tedge/mappers/`, including the directory layout, the `mapper.toml` configuration file format, and how mapper config values are exposed to bridge rule templates via the `${mapper.*}` namespace.

## Requirements

### Requirement: Custom mapper directory layout
A mapper SHALL be defined by a directory under `/etc/tedge/mappers/` containing a `mapper.toml` file. No prefix distinguishes built-in from user-defined mappers — the directory structure is identical for both. The directory MAY also contain a `bridge/` subdirectory with bridge rule TOML files and a `flows/` subdirectory with flow scripts. An empty directory is also valid on disk, but invoking it is an error (see runtime spec).

Mapper names SHALL match `[a-z][a-z0-9-]*` (lowercase ASCII letters, digits, and hyphens; must start with a letter). A mapper name containing an underscore SHALL be rejected with a hard error at startup. This restriction enables bijective mapping to environment variable names.

Directories under `/etc/tedge/mappers/` that contain no `mapper.toml` are not recognised as mappers and generate a warning (under the no-prefix D1 approach) or are matched by name pattern (under the `+` prefix approach).

#### Scenario: User-defined mapper recognised by mapper.toml presence
- **WHEN** a user creates `/etc/tedge/mappers/thingsboard/mapper.toml`
- **THEN** `tedge-mapper thingsboard` SHALL recognise `thingsboard` as a valid mapper and start

#### Scenario: Full user-defined mapper with bridge and flows
- **WHEN** a user creates `/etc/tedge/mappers/thingsboard/` containing `mapper.toml`, `bridge/rules.toml`, and `flows/telemetry.js`
- **THEN** the mapper SHALL load the configuration, bridge rules, and flow scripts from their respective locations

#### Scenario: Bridge rules without mapper.toml
- **WHEN** a user creates `/etc/tedge/mappers/thingsboard/bridge/rules.toml` but no `mapper.toml`
- **THEN** `tedge-mapper thingsboard` SHALL report an error indicating that bridge rules require connection settings (`mapper.toml`)

#### Scenario: Built-in and user-defined mappers coexist as peers
- **WHEN** `/etc/tedge/mappers/` contains `c8y/` (pre-installed, with `mapper.toml`) and `thingsboard/` (user-created, with `mapper.toml`)
- **THEN** both directories SHALL be recognised as valid mapper directories with equal standing

#### Scenario: Mapper name with underscore rejected
- **WHEN** a user runs `tedge-mapper my_cloud`
- **THEN** `tedge-mapper` SHALL exit immediately with a hard error indicating that underscore is not permitted in mapper names

#### Scenario: Mapper name starting with digit rejected
- **WHEN** a user runs `tedge-mapper 2cloud`
- **THEN** `tedge-mapper` SHALL exit immediately with a hard error indicating the name is invalid

### Requirement: Custom mapper configuration file
When present, a mapper's `mapper.toml` SHALL contain connection and device identity settings needed to establish the MQTT bridge to the cloud, plus an optional `cloud_type` field identifying the built-in cloud integration this mapper instance targets. The `mapper.toml` is OPTIONAL — it is only required when the mapper establishes a cloud connection via the built-in MQTT bridge.

For user-defined mappers, `mapper.toml` is parsed directly; it is not part of the global `tedge_config` schema. For built-in mappers (`c8y`, `az`, `aws`), `mapper.toml` is backed by `TEdgeConfig`; values are written via `tedge config set` as before.

#### Scenario: Configuration with connection details
- **WHEN** a mapper's `mapper.toml` contains a top-level `url` field in `{host}:{port}` format, and a `[device]` section with `cert_path` and `key_path` fields
- **THEN** the mapper SHALL use these values to configure the MQTT bridge connection

#### Scenario: Configuration with additional custom fields
- **WHEN** a mapper's `mapper.toml` contains additional TOML keys beyond the required connection and device settings (e.g. `[bridge]` with `topic_prefix`)
- **THEN** the mapper SHALL make all fields available via the `${mapper.*}` template namespace in bridge rule templates

#### Scenario: Invalid TOML in configuration file
- **WHEN** a mapper's `mapper.toml` contains invalid TOML syntax
- **THEN** `tedge-mapper` SHALL report a parse error with the file path and error location

#### Scenario: cloud_type field present in user-defined mapper
- **WHEN** a user-defined mapper's `mapper.toml` contains `cloud_type = "c8y"`
- **THEN** `tedge mapper list` SHALL display `cloud_type=c8y` alongside that mapper's name

#### Scenario: cloud_type field absent
- **WHEN** a mapper's `mapper.toml` does not contain a `cloud_type` field
- **THEN** `tedge mapper list` SHALL display the mapper without a cloud type annotation

#### Scenario: Built-in mapper.toml pre-populated with cloud_type
- **WHEN** the built-in `c8y` mapper directory is inspected
- **THEN** its `mapper.toml` SHALL contain `cloud_type = "c8y"` as pre-populated by packaging

### Requirement: Custom mapper config is separate from global tedge_config
User-defined mapper configuration SHALL NOT be part of the `define_tedge_config!` macro or the global `tedge_config` schema. Users SHALL configure user-defined mappers by editing the mapper's `mapper.toml` directly or via `tedge mapper config get` (read-only in this change). The `tedge config set/get` commands apply only to built-in mapper config backed by `TEdgeConfig`.

#### Scenario: tedge config does not see user-defined mapper settings
- **WHEN** a user runs `tedge config list`
- **THEN** the output SHALL NOT include any settings from user-defined mapper `mapper.toml` files

#### Scenario: User edits user-defined mapper config directly
- **WHEN** a user edits `/etc/tedge/mappers/thingsboard/mapper.toml` to change `url`
- **THEN** the change takes effect the next time the mapper is started (no `tedge config set` needed)

### Requirement: Bridge templates support mapper config namespace
The bridge template system SHALL support a `${mapper.*}` variable namespace that resolves against the mapper's own `mapper.toml`. This namespace is available in string template fields (`local_prefix`, `remote_prefix`, and `topic`) within bridge rule TOML files located in the mapper's `bridge/` directory. It is only populated when a `mapper.toml` is present.

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

#### Scenario: `${mapper.*}` without mapper.toml present
- **WHEN** a bridge rule template contains `${mapper.some.key}` but no `mapper.toml` is present for the mapper
- **THEN** the template system SHALL report an error indicating that `${mapper.*}` requires a `mapper.toml`

#### Scenario: `${mapper.*}` rejected in `if` conditions
- **WHEN** a bridge rule file contains `if = "${mapper.some_flag}"`
- **THEN** the template system SHALL report a clear parse error — `${mapper.*}` is not a valid condition expression; only `${config.*}` boolean references and `${connection.auth_method} == '...'` are accepted

#### Scenario: `${mapper.*}` rejected in `for` loop sources
- **WHEN** a bridge rule file contains a `[[template_rule]]` with `for = "${mapper.some_list}"`
- **THEN** the template system SHALL report a clear parse error — `${mapper.*}` is not a valid loop source; only `${config.*}` template set references and literal TOML arrays are accepted

#### Out of scope: Built-in mappers and the mapper namespace
`${mapper.*}` is only populated for user-defined mappers, whose config is a raw TOML table. Supporting `${mapper.*}` in built-in bridge rules is deferred to a future change if demand warrants it.
