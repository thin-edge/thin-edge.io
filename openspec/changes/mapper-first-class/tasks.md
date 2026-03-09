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
- [x] 3.6 Error with a list of available mappers when the directory or `mapper.toml` is not found
- [x] 3.7 Add unit tests: valid name, invalid name (underscore, uppercase, empty), unknown mapper, extra arguments

## 4. `CustomMapper` struct update **[replaces gm-4.1, gm-4.2]**

- [x] 4.1 Replace `profile: Option<ProfileName>` field with `name: String`
- [x] 4.2 Update `mapper_dir()` to construct the directory path using the new convention (no-prefix)
- [x] 4.3 Update all call sites and doc comments that reference `profile` or `custom.{name}`

## 5. Service identity **[replaces gm-5.x]**

- [x] 5.1 Derive service name as `tedge-mapper@{name}` (replaces `tedge-mapper-custom@{profile}`)
- [x] 5.2 Derive health topic as `te/device/main/service/tedge-mapper@{name}/status/health`
- [x] 5.3 Derive lock file path as `/run/tedge-mapper@{name}.lock`
- [x] 5.4 Derive bridge service name as `tedge-mapper-bridge-{name}` (note: uses `-` separator, not `@`, consistent with linter output)
- [x] 5.5 Update unit tests for all derived names

## 6. Directory scanner **[replaces gm-6.x]**

Tasks 6.1–6.3 are **D1-dependent**:

**No-prefix approach:**
- [x] 6.1a Rewrite scanner to classify directories by `mapper.toml` presence: directory contains `mapper.toml` → mapper; otherwise → unrecognised
- [x] 6.2a Read `cloud_type` from `mapper.toml` when present and include it in the classification result
- [x] 6.3a Emit a warning for any directory under `/etc/tedge/mappers/` that does not contain `mapper.toml`

**`+` prefix approach:**
- [ ] 6.1b Update scanner name-pattern rules: `+{name}` → user-defined; built-in names unchanged; `{builtin}.{profile}` → profile of built-in; anything else → unrecognised
- [ ] 6.2b _(cloud_type reading not needed for classification)_
- [ ] 6.3b Emit a warning for directories not matching any known pattern

**Common (both approaches):**
- [x] 6.4 Add unit tests: recognised built-in, recognised user-defined, unrecognised directory produces warning, `cloud_type` is correctly read/reported (no-prefix only)

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

- [ ] 9.1 Update the custom mapper directory layout documentation to use the new convention
- [ ] 9.2 Document `cloud_type` field: purpose, valid values, note that dispatch is not yet implemented
- [ ] 9.3 Document `tedge mapper list` and `tedge mapper config get` commands
- [ ] 9.4 Update the ThingsBoard walkthrough example (from `generic-mapper` docs) to use the new directory layout and CLI invocation
