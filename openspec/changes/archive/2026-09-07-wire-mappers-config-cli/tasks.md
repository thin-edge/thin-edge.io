## 1. New crate: `tedge_mapper_config`

- [x] 1.1 Scaffold `crates/common/tedge_mapper_config/` with `Cargo.toml`, add as workspace member. Dependencies: `tedge_config_engine`, `tedge_config` (for `HostPort`, `SecondsOrHumanTime`, `Utf8PathBuf`), `serde`, `facet`.
- [x] 1.2 Define `AuthMethod` enum (`Auto`, `Certificate`, `Password`) with `FromStr`/`Display`/`Serialize`/`Deserialize`. Add a test asserting it accepts the same string values as `AuthMethodConfig` in `crates/core/tedge_mapper/src/custom/config.rs`.
- [x] 1.3 Define mapper schema via `define_config!`: `url`, `device.{id, cert_path, key_path, root_cert_path}`, `bridge.{clean_session, keepalive_interval, tls, max_payload_size}`, `auth_method`, `credentials_path`. Use `from_root` on `device.cert_path`, `device.key_path`, `device.root_cert_path`.
- [x] 1.4 Define root config stub via `define_config!`: `device.cert_path`, `device.key_path`, `device.root_cert_path` with defaults matching the real `tedge.toml` schema. Add a test asserting stub defaults match `TEdgeConfigReader` values.
- [x] 1.5 Implement `build_federated_config(config_dir: &Path) -> Result<FederatedConfig, ConfigError>`: mount root stub at `""` (reading `tedge.toml`), scan `{config_dir}/mappers/` subdirectories, mount `TypedConfigOps<MapperDto>` at `mappers.<name>.` for each with a `mapper.toml`.
- [x] 1.6 Add environment variable override support: `apply_env_with_prefix("TEDGE_CONFIG_MAPPERS_{NAME}_", ...)` per mapper, `apply_env_excluding(["mappers."])` on root stub.

## 2. Schema validation tests

- [x] 2.1 Test that mapper schema `from_root` references resolve against the root stub (mount both in a `FederatedConfig`, read `mappers.test.device.cert_path`).
- [x] 2.2 Test that `build_federated_config` discovers mapper directories and mounts them correctly (use a temp dir with two mapper subdirectories).
- [x] 2.3 Test that unknown mapper names produce `ConfigError::UnknownMapper` listing known mappers.
- [x] 2.4 Test that unknown keys within a known mapper produce `ConfigError::UnknownKey`.
- [x] 2.5 Test environment variable overrides: set `TEDGE_CONFIG_MAPPERS_THINGSBOARD_URL`, verify it takes precedence over the TOML value.
- [x] 2.6 Test schema drift guard: assert the set of keys in the `define_config!` mapper schema matches the key set of `CustomMapperConfig`.

## 3. CLI routing: `FederatedReadableKey` / `FederatedWritableKey`

- [x] 3.1 Define `FederatedReadableKey` and `FederatedWritableKey` wrapper enums in `crates/core/tedge/src/cli/config/`. `FromStr` tries the inner `ReadableKey`/`WritableKey` first, falls back to `Mapper(String)` for `mappers.*` when the `mapper-config` feature is enabled.
- [x] 3.2 Implement `completions()` on both wrapper enums: combine `ReadableKey::completions()` / `WritableKey::completions()` with mapper keys from `build_federated_config`, following the `mapper_config_key_completions()` pattern.
- [x] 3.3 Add the `mapper-config` feature to `crates/core/tedge/Cargo.toml` (default off), gating the `tedge_mapper_config` dependency and the `Mapper` variant.

## 4. Wire wrapper enums into CLI command handlers

- [x] 4.1 Update `tedge config get` to use `FederatedReadableKey`. Route `Mapper(key)` to `FederatedConfig::read()`.
- [x] 4.2 Update `tedge config set` to use `FederatedWritableKey`. Route `Mapper(key)` to `FederatedConfig::mutate(key, Action::Set(value))`.
- [x] 4.3 Update `tedge config unset` to use `FederatedWritableKey`. Route to `FederatedConfig::mutate(key, Action::Unset)`.
- [x] 4.4 Update `tedge config add` to use `FederatedWritableKey`. Route to `FederatedConfig::mutate(key, Action::Add(value))`.
- [x] 4.5 Update `tedge config remove` to use `FederatedWritableKey`. Route to `FederatedConfig::mutate(key, Action::Remove(value))`.
- [x] 4.6 Update `tedge config list` to append `FederatedConfig::all_entries()` when `mapper-config` is enabled.

## 5. Rust `#[test]` end-to-end tests (compiled with `--features mapper-config`)

- [x] 5.1 Test `tedge config get mappers.<name>.<key>` reads from `mapper.toml`.
- [x] 5.2 Test `tedge config set mappers.<name>.<key> <value>` writes to `mapper.toml`.
- [x] 5.3 Test `tedge config unset mappers.<name>.<key>` removes the key from `mapper.toml`.
- [x] 5.4 Test `tedge config list` includes `mappers.*` keys when the feature is enabled.
- [x] 5.5 Test `from_root` fallback: `tedge config get mappers.<name>.device.cert_path` returns the root `tedge.toml` value when unset in `mapper.toml`.
- [x] 5.6 Test error cases: unknown mapper name, unknown key, invalid value, non-existent mapper directory.
- [x] 5.7 Test that non-`mappers.*` keys are unaffected (existing behaviour preserved).
