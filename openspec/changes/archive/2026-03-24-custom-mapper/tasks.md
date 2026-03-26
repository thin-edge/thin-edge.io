## Bridge template system

- [x] Add `mapper_config: Option<&toml::Table>` to `TemplateContext`; register `mapper` as a keyword in the template parser
- [x] Resolve `${mapper.KEY}` by walking the TOML table (support nested keys); error clearly on missing key
- [x] Unit and integration tests: simple key, nested key, missing key, combined `${mapper.*}` + `${config.*}`, parse errors in `if`/`for` fields

## Custom mapper config

- [x] Define `CustomMapperConfig` with top-level `url: HostPort`; name config file `mapper.toml`
- [x] Add `cloud_type: Option<CloudType>` field; hard-error on unknown values
- [x] Resolve relative path fields against the mapper directory at parse time
- [x] Fall back to root `tedge.toml` for `device.cert_path`/`device.key_path` when absent from `mapper.toml`
- [x] Rename `tedge.toml` → `mapper.toml` in all built-in mapper directories (packaging, test fixtures, docs)
- [x] Unit tests: valid config, invalid TOML, file not found, relative paths, cert fallback

## CLI

- [x] Replace `Custom { profile }` with `#[clap(external_subcommand)] UserDefined(Vec<String>)`
- [x] Validate name: `[a-z][a-z0-9-]*`, reject names starting with `bridge-`, reject extra arguments
- [x] Error with list of available mappers when directory not found
- [x] Unit tests: valid name, invalid name, unknown mapper, extra arguments

## Startup validation

- [x] Implement `validate_and_load` returning `MapperStartup` enum; remove split validation functions
- [x] Error when neither `bridge/` nor `flows/` present; error when `bridge/` present but `mapper.toml` absent
- [x] Extract `build_cloud_mqtt_options` (auth method resolution, TLS config) into a pure synchronous function
- [x] Extract `build_flows_actors` helper; verify `start()` reads as a linear sequence of named steps
- [x] Unit tests for `build_cloud_mqtt_options`: all auth method combinations, `device.id` override, missing URL

## Service identity

- [x] Derive service name as `tedge-mapper-{name}`, health topic, lock file, bridge service name
- [x] Reject names starting with `bridge-` at startup
- [x] Remove `tedge-mapper@.service` template unit from packaging and nfpm manifest
- [x] Update Robot Framework tests to start mappers via `tedge-mapper <name>` rather than `systemctl start tedge-mapper@{name}`
- [x] Unit tests for all derived names

## Directory scanner

- [x] Classify directories by `mapper.toml` presence; read `cloud_type` from present files
- [x] Suppress warnings for built-in names and `{builtin}.{anything}` profile variants
- [x] Unit tests: recognised built-in, recognised user-defined, unrecognised triggers warning, `c8y-extra` is still flagged

## `tedge mapper` CLI

- [x] Add `Mapper` variant to `TEdgeOpt` with `list` and `config get <name>.<key>` subcommands
- [x] `list`: scan for `mapper.toml` directories, print name and `cloud_type`
- [x] `config get`: split on first `.`, locate mapper dir, read and walk `mapper.toml`
- [x] Unit tests: empty directory, mixed mappers, valid/invalid key paths, mapper not found

## Integration tests & documentation

- [x] Robot Framework tests: flows-only mapper, bridge+config mapper, concurrent profiles
- [x] Document directory layout, `mapper.toml` schema, `${mapper.*}` namespace, `tedge mapper` commands
- [x] Update ThingsBoard walkthrough for new directory convention and CLI invocation
