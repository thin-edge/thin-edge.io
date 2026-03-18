Tasks are grouped by area. Where tasks depend on the D1 decision (no-prefix vs `+` prefix), this is noted explicitly. Tasks that replace work from `generic-mapper` are marked **[replaces gm-N.N]**.

## 1. Config file rename (`tedge.toml` → `mapper.toml`)

- [x] 1.1 Rename `tedge.toml` to `mapper.toml` in all built-in mapper directories in the repository (packaging, test fixtures, documentation)
- [x] 1.2 Update `load_mapper_config` in `custom/config.rs` to read from `mapper.toml` instead of `tedge.toml`
- [x] 1.3 Update `populate_mapper_configs` (and any other code that reads per-mapper `tedge.toml` files for built-in mappers) to use `mapper.toml`
- [x] 1.4 Update all unit and integration test fixtures that reference `tedge.toml` inside mapper directories
- [x] 1.5 Add a config parsing test that verifies `mapper.toml` is read (and `tedge.toml` is not) for user-defined mappers

## 2. `cloud_type` field

- [x] 2.1 Add `cloud_type: Option<CloudType>` to `CustomMapperConfig` (and the built-in mapper DTO via `#[tedge_config(reader(skip))]`)
- [x] 2.2 Populate `cloud_type` in built-in mapper configs via `migrate_mapper_config` (writes `cloud_type = "c8y"` etc. alongside `mapper_config_dir`)
- [x] 2.3 Add unit tests for config parsing: `cloud_type` present, `cloud_type` absent, unknown `cloud_type` value (errors — decided to hard-error on unknown values rather than warn)

## 3. CLI: replace `Custom { profile }` with `external_subcommand` **[replaces gm-3.x]**

- [x] 3.1 Remove `Custom { profile: Option<ProfileName> }` variant from `MapperName`
- [x] 3.2 Add `#[clap(external_subcommand)] UserDefined(Vec<String>)` variant
- [x] 3.3 Implement name extraction: take the first element of the `Vec` as the mapper name; reject extra elements with a clear error
- [x] 3.4 Validate the mapper name matches `[a-z][a-z0-9-]*`; reject with a hard error otherwise
- [x] 3.5 Resolve the mapper directory from the name (no-prefix: `{name}/`)
- [x] 3.6 Error with a list of available mappers when the mapper directory is not found
- [x] 3.7 Add unit tests: valid name, invalid name (underscore, uppercase, empty), unknown mapper, extra arguments

## 4. `CustomMapper` struct update **[replaces gm-4.1, gm-4.2]**

- [x] 4.1 Replace `profile: Option<ProfileName>` field with `name: String`
- [x] 4.2 Update `mapper_dir()` to construct the directory path using the new convention (no-prefix)
- [x] 4.3 Update all call sites and doc comments that reference `profile` or `custom.{name}`

## 5. Service identity **[replaces gm-5.x]**

- [x] 5.1 Derive service name as `tedge-mapper-{name}` (was `tedge-mapper@{name}`)
- [x] 5.2 Derive health topic as `te/device/main/service/tedge-mapper-{name}/status/health`
- [x] 5.3 Derive lock file path as `/run/tedge-mapper-{name}.lock`
- [x] 5.4 Derive bridge service name as `tedge-mapper-bridge-{name}`
- [x] 5.5 Update unit tests for all derived names
- [x] 5.6 Add validation that mapper names starting with `bridge-` are rejected with a hard error
- [x] 5.7 Remove `tedge-mapper@.service` systemd template unit from `configuration/init/systemd/`
- [x] 5.8 Remove `tedge-mapper@.service` entries from `configuration/package_manifests/nfpm.tedge-mapper.yaml`
- [x] 5.9 Update Robot Framework system tests in `tests/RobotFramework/tests/custom_mapper/` to start mappers directly (e.g. via `tedge-mapper <name>` or a plain service file) rather than via `systemctl start tedge-mapper@{name}`

## 6. Directory scanner **[replaces gm-6.x]**

- [x] 6.1 Rewrite scanner to classify directories by `mapper.toml` presence: directory contains `mapper.toml` → mapper; otherwise → unrecognised
- [x] 6.2 Read `cloud_type` from `mapper.toml` when present and include it in the classification result
- [x] 6.3 Emit a warning for any directory under `/etc/tedge/mappers/` that does not contain `mapper.toml`
- [x] 6.4 Add unit tests: recognised built-in, recognised user-defined, unrecognised directory produces warning, `cloud_type` is correctly read/reported

## 7. `tedge mapper` subcommand (new)

- [x] 7.1 Add `Mapper` variant to `TEdgeOpt` in `crates/core/tedge/src/cli/mod.rs`
- [x] 7.2 Implement `tedge mapper list`: scan `/etc/tedge/mappers/` for mapper directories (by `mapper.toml` presence), read `cloud_type` from each, print name and `cloud_type` (or `(none)`)
- [x] 7.3 Add unit tests for `tedge mapper list`: empty directory, built-in only, mixed built-in and user-defined
- [x] 7.4 Implement `tedge mapper config get <name>.<key>`: split on first `.` to extract mapper name and TOML key path; locate mapper directory; read and walk `mapper.toml` using the key path
- [x] 7.5 Print the raw value to stdout (matching `tedge config get` behaviour)
- [x] 7.6 Add unit tests for `tedge mapper config get`: valid key, top-level key, nested key (`device.cert_path`), mapper not found, `mapper.toml` not found, key not found, non-table intermediate node

## 8. Code cleanup

- [x] 8.1 Remove all references to `custom.{name}` directory naming in code, comments, and doc strings
- [x] 8.2 Remove `ProfileName` from `CustomMapper` and all imports that become unused
- [x] 8.3 Update the error message in `check_startup_config` (currently references the old `custom.` directory convention)
- [x] 8.4 Update `warn_unrecognised_mapper_dirs` to use the new scanner logic from task 6

## 9. Documentation

- [x] 9.1 Update the custom mapper directory layout documentation to use the new convention
- [x] 9.2 Document `cloud_type` field: purpose, valid values, note that dispatch is not yet implemented
- [x] 9.3 Document `tedge mapper list` and `tedge mapper config get` commands
- [x] 9.4 Update the ThingsBoard walkthrough example (from `generic-mapper` docs) to use the new directory layout and CLI invocation

## 10. OQ1 — suppress false-positive warnings for built-in mapper directories

Built-in mappers (`c8y`, `az`, `aws`, `collectd`, `local`) legitimately have no `mapper.toml` (their config lives in the root `tedge.toml`). Their subdirectories exist at runtime because the mapper creates `flows/` and `bridge/` under them. The current `warn_unrecognised_mapper_dirs` warns about any directory without `mapper.toml`, producing false positives for these built-in directories and their profile variants (e.g. `c8y.prod`).

- [x] 10.1 Add `pub(crate) fn is_builtin_mapper_dir_name(name: &str) -> bool` in `lib.rs` (alongside `MapperName`), recognising the exact names `c8y`, `az`, `aws`, `collectd`, `local` and any `{builtin}.{anything}` profile variant (matched via `starts_with("{builtin}.")`)
- [x] 10.2 Update `collect_unrecognised_mapper_dirs` in `mappers_dir.rs` to skip directories where `is_builtin_mapper_dir_name` returns `true`
- [x] 10.3 Add/update tests:
  - a built-in dir without `mapper.toml` (e.g. `c8y/`) is not flagged
  - a profiled built-in dir without `mapper.toml` (e.g. `c8y.prod/`) is not flagged
  - a user-defined dir without `mapper.toml` (e.g. `thingsboard/`) is still flagged
  - a name that starts with a builtin prefix but is not a profile (e.g. `c8y-extra/`) is still flagged

## 11. OQ2 — inherit `device.cert_path` / `device.key_path` from root `tedge.toml`

User-defined mappers using certificate TLS currently require `device.cert_path` and `device.key_path` in `mapper.toml`. When these are absent, `build_cloud_mqtt_options` silently skips client-cert setup (the `if let Some(device)` branch does nothing). The mapper should fall back to the values already configured in the root `tedge.toml`.

- [x] 11.1 In `build_cloud_mqtt_options` (`custom/mapper.rs`), when `config.device.cert_path` / `device.key_path` are absent, read the fallback values from `tedge_config.device.cert_path` and `tedge_config.device.key_path`; precedence: `mapper.toml` > root `tedge.toml`
- [x] 11.2 Add unit tests:
  - explicit `mapper.toml` values are used when present (no fallback)
  - absent `mapper.toml` values fall back to `TEdgeConfig` values
  - both absent and `TEdgeConfig` also absent → existing error behaviour unchanged
- [x] 11.3 Update the `CustomMapperConfig` doc comment to document the fallback behaviour

## 12. OQ3 — relative paths in `mapper.toml`

All path fields in `CustomMapperConfig` (`device.cert_path`, `device.key_path`, `device.root_cert_path`, `credentials_path`) are currently treated as absolute. Users who want to store a cloud-specific cert next to `mapper.toml` must use an absolute path. Relative paths should be resolved relative to the mapper directory at load time so the rest of the code always sees absolute paths.

- [x] 12.1 After deserialising `CustomMapperConfig` in `load_mapper_config` (`custom/config.rs`), resolve each of `device.cert_path`, `device.key_path`, `device.root_cert_path`, `credentials_path` relative to the mapper directory if it is a relative path; leave absolute paths unchanged
- [x] 12.2 Add unit tests:
  - a relative `cert_path = "cert.pem"` is resolved to `{mapper_dir}/cert.pem`
  - an absolute path is returned unchanged
  - nested relative path (`device/cert.pem`) resolves correctly
- [x] 12.3 Document the relative-path behaviour in the `CustomMapperConfig` struct doc comment and in the `mapper.toml` schema documentation
