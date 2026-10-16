## 1. Writes refuse files with unknown keys

- [x] 1.1 Add `toml` as a dependency of `tedge_config_engine`
- [x] 1.2 Add a `ConfigError` variant for unknown keys in a file, naming the file and every unknown key
- [x] 1.3 In `TypedConfigOps::mutate`, read the file as plain TOML, compare its keys with the schema's keys, and return the error before writing if any are unknown
- [x] 1.4 Tests: set is refused and the file is unchanged when it has `[smartrest] templates`; get still works on that file; set works when every key is known

## 2. `cloud_type` key

- [x] 2.1 Add `CloudType { C8y, Az, Aws }` in `tedge_mapper_config/src/cloud_type.rs`, parsed and printed in lowercase, following `auth_method.rs`
- [x] 2.2 Add `tedge_config` as a dev-dependency and a test that `CloudType` accepts, prints and rejects the same strings as `tedge_config::models::CloudType`
- [x] 2.3 Add `cloud_type: CloudType` with a doc comment to the `Mapper` schema
- [x] 2.4 Tests: set and get `cloud_type`; an unknown value is rejected naming `c8y`, `az` and `aws`; setting it leaves other keys and the key list unchanged

## 3. Key list test against `tedge_mapper`

- [x] 3.1 Add `tedge_mapper_config` as a dev-dependency of `tedge_mapper`
- [x] 3.2 Add a test in `tedge_mapper` asserting the `Mapper` schema keys equal `custom_mapper_schema_keys()` plus `cloud_type`
- [x] 3.3 Delete `mapper_schema_keys_match_custom_mapper_config` from `tedge_mapper_config/src/tests.rs`
- [x] 3.4 Fix the drift the test found: production reads `bridge.tls.enable` (`true`, `false`, `auto`), not `bridge.tls`; add a `TlsEnable` model with a parity test against `BridgeTlsEnable`

## 4. Profile keys under `profiles`

- [x] 4.1 Add a mount prefix function: `<cloud>.<profile>` for a built-in cloud gives `mappers.<cloud>.profiles.<profile>.`, any other directory gives `mappers.<dir>.`
- [x] 4.2 Use it in `load_federated_config` to mount each mapper directory
- [x] 4.3 Replace the split-guessing in `extract_builtin_mapper_name` with parsing `mappers.<cloud>.profiles.<profile>.` and `mappers.<cloud>.`, and update `ensure_builtin_mapper_dir` to match
- [x] 4.4 Update `tedge mapper config get` to turn `<cloud>.<profile>.<key>` into `mappers.<cloud>.profiles.<profile>.<key>` when `mappers/<cloud>.<profile>/` exists
- [x] 4.5 Add a test that no cloud schema has a top-level group named `profiles`
- [x] 4.6 Tests: set a profile key; `mappers.c8y.bridge.clean_session` writes to `c8y` while `mappers/c8y.bridge/` exists; `mappers.c8y.prod.url` is an unknown key and creates no directory; `tedge mapper config get c8y.prod.url` reads the profile
- [x] 4.7 Update the existing tests that use `mappers.c8y.prod.*` keys, such as `creates_profiled_mapper_directory`

## 5. Environment variable names

- [x] 5.1 Build each mapper's env prefix from its mount prefix: `TEDGE_` plus the uppercased prefix, with `.` and `-` replaced by `_`
- [x] 5.2 Add a `ConfigError` variant for two directories with the same env prefix, naming both and the prefix, and return it from every read and write of both colliding mappers
- [x] 5.3 Delete the root config's `apply_env_excluding` call, which does nothing because the root has no env prefix
- [x] 5.4 Tests: `TEDGE_MAPPERS_C8Y_PROFILES_PROD_URL` overrides `mappers.c8y.profiles.prod.url`; `thingsboard.eu` and `thingsboard-eu` together make both fail while other mappers still load; the existing `thingsboard` override still works
- [x] 5.5 Skip `TEDGE_MAPPERS_*` in the old engine's environment loop, so it doesn't warn about them as unknown keys

## 6. A broken mapper file only affects that mapper

- [x] 6.1 Tests: set on another mapper succeeds while one file is invalid TOML or holds an invalid value; get and set on the broken mapper fail naming its file
- [x] 6.2 `TypedConfigOps::new` keeps a load error instead of returning it; `get` and `read` return it
- [x] 6.3 `tedge config list` warns once per mapper file that didn't parse, and lists the other mappers

## 7. Written files and value errors

- [x] 7.1 Write `mapper.toml` groups as `[group]` table sections, not inline tables
- [x] 7.2 Test: setting `mappers.thingsboard.bridge.tls.enable` writes a `[bridge.tls]` section
- [x] 7.3 Errors for a value a key doesn't accept name the key and list the accepted values, whether the value is set or read from the file
- [x] 7.4 Tests: setting `bridge.tls.enable` to `on` lists `true`, `false` and `auto`; an invalid value in `mapper.toml` is reported with its key

## 8. Finish

- [x] 8.1 `cargo nextest run -p tedge_config_engine -p tedge_mapper_config -p tedge_mapper` and `cargo nextest run -p tedge --features mapper-config`
- [x] 8.2 `just check` and `cargo +nightly fmt`
