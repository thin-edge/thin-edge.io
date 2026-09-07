## ADDED Requirements

### Requirement: tedge config get for mapper keys
`tedge config get mappers.<name>.<key>` SHALL read the value
from the named mapper's `mapper.toml` via the `FederatedConfig` engine.
If the key is not explicitly set in `mapper.toml`
but has a `from_root` default (e.g. `device.cert_path`),
the value SHALL be resolved from the root `tedge.toml`.
If neither source has a value and the key has a static default,
the static default SHALL be returned.
If no value can be resolved, the command SHALL print nothing to stdout
and exit with code 0 (matching existing `tedge config get` behaviour for unset keys).

#### Scenario: Read an explicitly set mapper key
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "mqtt.thingsboard.io:8883"`
  and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** stdout SHALL contain `mqtt.thingsboard.io:8883`

#### Scenario: Read a mapper key that falls back to root config
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` does not set `device.cert_path`
  and `tedge.toml` has `device.cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"`
  and a user runs `tedge config get mappers.thingsboard.device.cert_path`
- **THEN** stdout SHALL contain `/etc/tedge/device-certs/tedge-certificate.pem`

#### Scenario: Read a mapper key with a static default
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` does not set `device.cert_path`
  and `tedge.toml` also does not set `device.cert_path`
  and a user runs `tedge config get mappers.thingsboard.device.cert_path`
- **THEN** stdout SHALL contain the static default `/etc/tedge/device-certs/tedge-certificate.pem`

#### Scenario: Read a mapper key with no value
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` does not set `url`
  and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** stdout SHALL be empty and the exit code SHALL be 0

### Requirement: tedge config set for mapper keys
`tedge config set mappers.<name>.<key> <value>` SHALL parse the value
according to the key's type and write it to the named mapper's `mapper.toml`.
The file SHALL be created if it does not exist.
The mapper directory SHALL already exist.

#### Scenario: Set a mapper key
- **WHEN** a user runs `tedge config set mappers.thingsboard.url mqtt.thingsboard.io:8883`
- **THEN** `/etc/tedge/mappers/thingsboard/mapper.toml` SHALL contain `url = "mqtt.thingsboard.io:8883"`

#### Scenario: Set a nested mapper key
- **WHEN** a user runs `tedge config set mappers.thingsboard.bridge.clean_session true`
- **THEN** `/etc/tedge/mappers/thingsboard/mapper.toml` SHALL contain
  a `[bridge]` section with `clean_session = true`

#### Scenario: Set rejects an invalid value
- **WHEN** a user runs `tedge config set mappers.thingsboard.bridge.clean_session notabool`
- **THEN** the command SHALL exit with an error indicating the value could not be parsed as a boolean

#### Scenario: Set into a non-existent mapper directory
- **WHEN** no `/etc/tedge/mappers/thingsboard/` directory exists
  and a user runs `tedge config set mappers.thingsboard.url mqtt.thingsboard.io:8883`
- **THEN** the command SHALL exit with an error listing the known mapper directories

### Requirement: tedge config unset for mapper keys
`tedge config unset mappers.<name>.<key>` SHALL remove the key
from the named mapper's `mapper.toml`.
If the key was not set, the command SHALL succeed silently.

#### Scenario: Unset a mapper key
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "mqtt.thingsboard.io:8883"`
  and a user runs `tedge config unset mappers.thingsboard.url`
- **THEN** the `url` key SHALL be removed from `mapper.toml`

#### Scenario: Unset a key that was not set
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` does not contain `url`
  and a user runs `tedge config unset mappers.thingsboard.url`
- **THEN** the command SHALL exit with code 0

### Requirement: tedge config list includes mapper keys
When the `mapper-config` feature is enabled,
`tedge config list` SHALL append keys from all discovered mapper configs
after the existing core config keys.
For each mapper directory under `{config_dir}/mappers/` that contains a `mapper.toml`,
all schema-defined keys SHALL be listed with the `mappers.<name>.` prefix.

#### Scenario: List includes mapper keys
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` exists
  and a user runs `tedge config list`
- **THEN** the output SHALL include `mappers.thingsboard.url`,
  `mappers.thingsboard.device.cert_path`, and all other mapper schema keys

#### Scenario: List with no mapper directories
- **WHEN** `/etc/tedge/mappers/` contains no subdirectories with `mapper.toml`
  and a user runs `tedge config list`
- **THEN** no `mappers.*` keys SHALL appear in the output

#### Scenario: List with multiple mappers
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` and `/etc/tedge/mappers/custom/mapper.toml` exist
- **THEN** `tedge config list` SHALL include keys prefixed with both
  `mappers.thingsboard.` and `mappers.custom.`

### Requirement: tedge config add and remove for mapper list keys
`tedge config add mappers.<name>.<key> <value>` and
`tedge config remove mappers.<name>.<key> <value>`
SHALL append to or remove from list-typed mapper keys.

#### Scenario: Add to a list-typed mapper key
- **WHEN** the mapper schema defines a list-typed key
  and a user runs `tedge config add mappers.thingsboard.<list_key> value`
- **THEN** the value SHALL be appended to the list in `mapper.toml`

#### Scenario: Remove from a list-typed mapper key
- **WHEN** the mapper schema defines a list-typed key containing `value`
  and a user runs `tedge config remove mappers.thingsboard.<list_key> value`
- **THEN** the value SHALL be removed from the list in `mapper.toml`

### Requirement: Environment variable overrides for mapper keys
Mapper config values SHALL be overridable via environment variables.
The variable name pattern is `TEDGE_CONFIG_MAPPERS_<NAME>_<KEY>`,
where `<NAME>` is the mapper name uppercased with hyphens replaced by underscores,
and `<KEY>` is the dotted key with dots replaced by underscores and uppercased.

#### Scenario: Override a mapper key via environment variable
- **WHEN** `TEDGE_CONFIG_MAPPERS_THINGSBOARD_URL=override.example.com:8883`
  is set and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** stdout SHALL contain `override.example.com:8883`

#### Scenario: Env override takes precedence over mapper.toml
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "file.example.com:8883"`
  and `TEDGE_CONFIG_MAPPERS_THINGSBOARD_URL=env.example.com:8883` is set
  and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** stdout SHALL contain `env.example.com:8883`

### Requirement: Unknown mapper error diagnostics
When a `mappers.*` key references a mapper name
that does not correspond to a subdirectory under `{config_dir}/mappers/`,
the command SHALL exit with an error that lists the known mapper directories.

#### Scenario: Unknown mapper name
- **WHEN** no `/etc/tedge/mappers/noexist/` directory exists
  and `/etc/tedge/mappers/thingsboard/` and `/etc/tedge/mappers/c8y/` exist
  and a user runs `tedge config get mappers.noexist.url`
- **THEN** the error message SHALL name `noexist` as unknown
  and SHALL list `thingsboard` and `c8y` as known mappers

### Requirement: Unknown key error diagnostics
When a `mappers.*` key references a valid mapper
but a key that is not in the mapper schema,
the command SHALL exit with an error indicating the key is unknown.

#### Scenario: Unknown key in a known mapper
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` exists
  and a user runs `tedge config get mappers.thingsboard.nonexistent`
- **THEN** the error message SHALL indicate that `nonexistent` is not a valid mapper config key

### Requirement: Feature flag gates mapper key routing
The `mappers.*` key routing SHALL only be active
when the `mapper-config` cargo feature is enabled on the `tedge` crate.
When the feature is disabled, `mappers.*` keys SHALL produce the existing
"unknown key" error from the `ReadableKey`/`WritableKey` enum.

#### Scenario: Feature disabled
- **WHEN** the `mapper-config` feature is not enabled
  and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** the command SHALL exit with an "unknown key" error

#### Scenario: Feature enabled
- **WHEN** the `mapper-config` feature is enabled
  and `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "mqtt.thingsboard.io:8883"`
  and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** stdout SHALL contain `mqtt.thingsboard.io:8883`
