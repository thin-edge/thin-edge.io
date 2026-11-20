## ADDED Requirements

### Requirement: Writes refuse files with unknown keys
`tedge config set`, `unset`, `add` and `remove` for `mappers.*` keys
SHALL fail without changing `mapper.toml`
if the file contains a key the mapper schema does not define.
The error SHALL name the file and every unknown key.
Reading keys from such a file SHALL keep working.

#### Scenario: Set is refused when the file has an unknown key
- **WHEN** `/etc/tedge/mappers/c8y/mapper.toml` contains `url = "a.example.com:8883"`
  and a `[smartrest]` table with `templates = ["x"]`
  and a user runs `tedge config set mappers.c8y.url b.example.com:8883`
- **THEN** the command SHALL exit with an error naming `/etc/tedge/mappers/c8y/mapper.toml` and `smartrest.templates`
  and `mapper.toml` SHALL be unchanged

#### Scenario: Get still works when the file has an unknown key
- **WHEN** `/etc/tedge/mappers/c8y/mapper.toml` contains `url = "a.example.com:8883"`
  and a `[smartrest]` table with `templates = ["x"]`
  and a user runs `tedge config get mappers.c8y.url`
- **THEN** stdout SHALL be `a.example.com:8883`

#### Scenario: Set works when every key is known
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "a.example.com:8883"`
  and a `[bridge]` table with `clean_session = true`
  and a user runs `tedge config set mappers.thingsboard.url b.example.com:8883`
- **THEN** `mapper.toml` SHALL contain `url = "b.example.com:8883"`
  and a `[bridge]` table with `clean_session = true`

### Requirement: Writes use table sections
`tedge config` writes to `mapper.toml` SHALL write each group of keys as a `[group]` table section,
not as an inline table.

#### Scenario: Nested keys are written as table sections
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` is empty
  and a user runs `tedge config set mappers.thingsboard.bridge.tls.enable false`
- **THEN** `mapper.toml` SHALL contain a `[bridge.tls]` table with `enable = "false"`

### Requirement: Value errors name the key and the accepted values
If a mapper key is given a value it does not accept,
either by `tedge config set` or in `mapper.toml`,
the error SHALL name the key and list the values it accepts.

#### Scenario: Set lists the accepted values
- **WHEN** a user runs `tedge config set mappers.thingsboard.bridge.tls.enable on`
- **THEN** the command SHALL exit with an error listing `true`, `false` and `auto`
  and `mapper.toml` SHALL be unchanged

#### Scenario: An invalid value in the file is reported with its key
- **WHEN** `/etc/tedge/mappers/broken/mapper.toml` contains a `[bridge]` table with `tls.enable = "on"`
  and a user runs `tedge config get mappers.broken.url`
- **THEN** the command SHALL exit with an error naming `bridge.tls.enable`
  and listing `true`, `false` and `auto`

### Requirement: A broken mapper file only affects that mapper
If a mapper's `mapper.toml` is not valid TOML or holds a value its key does not accept,
`tedge config` commands for that mapper's keys SHALL fail with an error naming the file,
and commands for every other mapper's keys SHALL work as if the file were absent.

#### Scenario: Set on another mapper succeeds
- **WHEN** `/etc/tedge/mappers/broken/mapper.toml` contains invalid TOML
  and a user runs `tedge config set mappers.thingsboard.url a.example.com:8883`
- **THEN** `/etc/tedge/mappers/thingsboard/mapper.toml` SHALL contain `url = "a.example.com:8883"`

#### Scenario: Get on the broken mapper names its file
- **WHEN** `/etc/tedge/mappers/broken/mapper.toml` contains invalid TOML
  and a user runs `tedge config get mappers.broken.url`
- **THEN** the command SHALL exit with an error naming `/etc/tedge/mappers/broken/mapper.toml`

#### Scenario: List warns and shows the other mappers
- **WHEN** `/etc/tedge/mappers/broken/mapper.toml` contains invalid TOML
  and `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "a.example.com:8883"`
  and a user runs `tedge config list`
- **THEN** stdout SHALL contain `mappers.thingsboard.url=a.example.com:8883`
  and a warning SHALL name `/etc/tedge/mappers/broken/mapper.toml`

### Requirement: cloud_type mapper key
Every mapper SHALL have a `cloud_type` key accepting `c8y`, `az` or `aws`.
Setting `cloud_type` SHALL change only the `cloud_type` value in `mapper.toml`.

#### Scenario: Set and get cloud_type
- **WHEN** a user runs `tedge config set mappers.thingsboard.cloud_type c8y`
  and then `tedge config get mappers.thingsboard.cloud_type`
- **THEN** `mapper.toml` SHALL contain `cloud_type = "c8y"`
  and stdout SHALL be `c8y`

#### Scenario: Set rejects an unknown cloud type
- **WHEN** a user runs `tedge config set mappers.thingsboard.cloud_type notacloud`
- **THEN** the command SHALL exit with an error naming the rejected value `notacloud`
  and `mapper.toml` SHALL be unchanged

#### Scenario: Setting cloud_type leaves other keys alone
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "a.example.com:8883"`
  and a user runs `tedge config set mappers.thingsboard.cloud_type c8y`
- **THEN** `mapper.toml` SHALL contain `url = "a.example.com:8883"` and `cloud_type = "c8y"`
  and `tedge config list` SHALL show the same `mappers.thingsboard.*` keys as before, plus `cloud_type`

### Requirement: Profile keys use a profiles segment
Keys of a profile of a built-in mapper (`c8y`, `az` or `aws`)
SHALL be `mappers.<cloud>.profiles.<profile>.<key>`,
stored in `mappers/<cloud>.<profile>/mapper.toml`.
`mappers.<cloud>.<x>.<key>` SHALL always mean key `<x>.<key>` of the base mapper `<cloud>`.
No cloud schema SHALL have a top-level group named `profiles`.

#### Scenario: Set a profile key
- **WHEN** a user runs `tedge config set mappers.c8y.profiles.prod.url example.com:8883`
- **THEN** `/etc/tedge/mappers/c8y.prod/mapper.toml` SHALL contain `url = "example.com:8883"`

#### Scenario: A profile named after a group does not shadow the group
- **WHEN** `/etc/tedge/mappers/c8y.bridge/mapper.toml` exists
  and a user runs `tedge config set mappers.c8y.bridge.clean_session true`
- **THEN** `/etc/tedge/mappers/c8y/mapper.toml` SHALL contain a `[bridge]` table with `clean_session = true`
  and `/etc/tedge/mappers/c8y.bridge/mapper.toml` SHALL be unchanged

#### Scenario: The old profile key shape is an unknown key
- **WHEN** a user runs `tedge config set mappers.c8y.prod.url example.com:8883`
- **THEN** the command SHALL exit with an unknown key error for `prod.url` of mapper `c8y`
  and `/etc/tedge/mappers/c8y.prod/` SHALL NOT be created

#### Scenario: tedge mapper config get reads profile keys
- **WHEN** `/etc/tedge/mappers/c8y.prod/mapper.toml` contains `url = "example.com:8883"`
  and a user runs `tedge mapper config get c8y.prod.url`
- **THEN** stdout SHALL be `example.com:8883`

## MODIFIED Requirements

### Requirement: Environment variable overrides for mapper keys
Mapper config values SHALL be overridable via environment variables.
The variable name pattern is `TEDGE_MAPPERS_<NAME>_<KEY>`,
where `<NAME>` is the mapper's key prefix after `mappers.` (`thingsboard`, `c8y.profiles.prod`)
uppercased with hyphens and dots replaced by underscores,
and `<KEY>` is the dotted key with dots replaced by underscores and uppercased.
If two mapper directories produce the same `<NAME>`,
every read and write of either mapper's keys SHALL fail with an error naming both directories,
and every other mapper's keys SHALL keep working.
`TEDGE_MAPPERS_*` variables SHALL NOT be reported as unknown `tedge.toml` keys.

#### Scenario: Override a mapper key via environment variable
- **WHEN** `TEDGE_MAPPERS_THINGSBOARD_URL=override.example.com:8883`
  is set and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** stdout SHALL contain `override.example.com:8883`

#### Scenario: Env override takes precedence over mapper.toml
- **WHEN** `/etc/tedge/mappers/thingsboard/mapper.toml` contains `url = "file.example.com:8883"`
  and `TEDGE_MAPPERS_THINGSBOARD_URL=env.example.com:8883` is set
  and a user runs `tedge config get mappers.thingsboard.url`
- **THEN** stdout SHALL contain `env.example.com:8883`

#### Scenario: Override a profiled mapper key
- **WHEN** `/etc/tedge/mappers/c8y.prod/` exists
  and `TEDGE_MAPPERS_C8Y_PROFILES_PROD_URL=prod.example.com:8883` is set
  and a user runs `tedge config get mappers.c8y.profiles.prod.url`
- **THEN** stdout SHALL contain `prod.example.com:8883`

#### Scenario: Mapper variables are not reported as unknown keys
- **WHEN** `TEDGE_MAPPERS_THINGSBOARD_URL=env.example.com:8883` is set
  and a user runs any `tedge` command
- **THEN** no warning SHALL report `mappers_thingsboard_url` as an unknown configuration field

#### Scenario: Directories with the same variable name are an error
- **WHEN** both `/etc/tedge/mappers/thingsboard.eu/` and `/etc/tedge/mappers/thingsboard-eu/` exist
  and a user runs `tedge config get mappers.thingsboard-eu.url`
- **THEN** the command SHALL exit with an error naming `thingsboard.eu`, `thingsboard-eu`
  and the prefix `TEDGE_MAPPERS_THINGSBOARD_EU_`

#### Scenario: Other mappers still work when two directories have the same variable name
- **WHEN** both `/etc/tedge/mappers/thingsboard.eu/` and `/etc/tedge/mappers/thingsboard-eu/` exist
  and `/etc/tedge/mappers/c8y/mapper.toml` contains `url = "c8y.example.com:8883"`
  and a user runs `tedge config get mappers.c8y.url`
- **THEN** stdout SHALL be `c8y.example.com:8883`
