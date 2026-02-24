## ADDED Requirements

### Requirement: Custom mapper directory layout
A custom mapper SHALL be defined by a directory under `/etc/tedge/mappers/` containing a `tedge.toml` configuration file. The directory MAY also contain a `bridge/` subdirectory with bridge rule TOML files and a `flows/` subdirectory with flow scripts. This layout mirrors the structure used by built-in mappers.

#### Scenario: Minimal custom mapper directory
- **WHEN** a user creates `/etc/tedge/mappers/_thingsboard/tedge.toml` with valid connection settings
- **THEN** `tedge-mapper` SHALL recognise `_thingsboard` as a valid custom mapper

#### Scenario: Full custom mapper directory with bridge and flows
- **WHEN** a user creates `/etc/tedge/mappers/_thingsboard/` containing `tedge.toml`, `bridge/rules.toml`, and `flows/telemetry.js`
- **THEN** the custom mapper SHALL load the configuration, bridge rules, and flow scripts from their respective locations

#### Scenario: Missing tedge.toml
- **WHEN** a user creates `/etc/tedge/mappers/_mycloud/` with a `bridge/` subdirectory but no `tedge.toml`
- **THEN** `tedge-mapper` SHALL report an error indicating that the configuration file is missing

### Requirement: Custom mapper names use underscore prefix
Custom mapper directory names MUST start with a `_` character. The `_` prefix distinguishes custom mappers from built-in mappers (which use plain names like `c8y`, `az`, `aws`) on the filesystem and in CLI invocation.

#### Scenario: Valid custom mapper name
- **WHEN** a directory named `_thingsboard` exists under `/etc/tedge/mappers/`
- **THEN** `tedge-mapper` SHALL treat it as a custom mapper directory

#### Scenario: Name without underscore prefix is not a custom mapper
- **WHEN** a directory named `thingsboard` (no `_` prefix) exists under `/etc/tedge/mappers/` and does not match any built-in mapper name or profile directory
- **THEN** `tedge-mapper` SHALL NOT treat it as a custom mapper and SHALL emit a warning about an unrecognised directory

#### Scenario: Name validation
- **WHEN** a user attempts to run a custom mapper whose name does not start with `_`
- **THEN** `tedge-mapper` SHALL report an error indicating that custom mapper names must start with `_`

### Requirement: Custom mapper configuration file
The custom mapper's `tedge.toml` SHALL contain connection and device identity settings needed to establish the MQTT bridge to the cloud. The configuration file is parsed directly by the custom mapper and is not part of the global `tedge_config` schema.

#### Scenario: Configuration with connection details
- **WHEN** a custom mapper's `tedge.toml` contains `[connection]` with `url` and `port` fields, and `[device]` with `cert_path` and `key_path` fields
- **THEN** the custom mapper SHALL use these values to configure the MQTT bridge connection

#### Scenario: Configuration with additional custom fields
- **WHEN** a custom mapper's `tedge.toml` contains additional TOML keys beyond the required connection and device settings (e.g. `[bridge]` with `topic_prefix`)
- **THEN** the custom mapper SHALL make all fields available via the `${mapper.*}` template namespace in bridge rule templates

#### Scenario: Invalid TOML in configuration file
- **WHEN** a custom mapper's `tedge.toml` contains invalid TOML syntax
- **THEN** `tedge-mapper` SHALL report a parse error with the file path and error location

### Requirement: Custom mapper config is separate from global tedge_config
Custom mapper configuration SHALL NOT be part of the `define_tedge_config!` macro or the global `tedge_config` schema. Users SHALL configure custom mappers by editing the mapper's `tedge.toml` directly, not via `tedge config set/get`.

#### Scenario: tedge config does not see custom mapper settings
- **WHEN** a user runs `tedge config list`
- **THEN** the output SHALL NOT include any settings from custom mapper `tedge.toml` files

#### Scenario: User edits custom mapper config directly
- **WHEN** a user edits `/etc/tedge/mappers/_thingsboard/tedge.toml` to change `connection.url`
- **THEN** the change takes effect the next time the custom mapper is started (no `tedge config set` needed)

### Requirement: Bridge templates support mapper config namespace
The bridge template system SHALL support a `${mapper.*}` variable namespace that resolves against the custom mapper's own `tedge.toml`. This namespace is available in bridge rule TOML files located in the mapper's `bridge/` directory.

#### Scenario: Referencing a mapper config value in a bridge template
- **WHEN** a bridge rule template contains `${mapper.bridge.topic_prefix}` and the mapper's `tedge.toml` contains `[bridge]` with `topic_prefix = "tb"`
- **THEN** the template SHALL expand to `tb`

#### Scenario: Referencing a nested mapper config value
- **WHEN** a bridge rule template contains `${mapper.connection.url}` and the mapper's `tedge.toml` contains `[connection]` with `url = "mqtt.thingsboard.io"`
- **THEN** the template SHALL expand to `mqtt.thingsboard.io`

#### Scenario: Referencing a non-existent mapper config key
- **WHEN** a bridge rule template contains `${mapper.nonexistent.key}` and no such key exists in the mapper's `tedge.toml`
- **THEN** the template system SHALL report an error indicating the key was not found

#### Scenario: Combining mapper and global config references
- **WHEN** a bridge rule template contains both `${mapper.bridge.topic_prefix}` and `${config.mqtt.port}`
- **THEN** both variables SHALL resolve correctly — `${mapper.*}` from the mapper's `tedge.toml` and `${config.*}` from the global thin-edge config

#### Scenario: Built-in mappers can use mapper namespace
- **WHEN** a built-in mapper's bridge rule template uses `${mapper.*}` to reference its own config values
- **THEN** the template SHALL resolve correctly against the built-in mapper's configuration (the existing `${config.*}` references continue to work as well)
