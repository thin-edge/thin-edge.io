# Facet Config Prototype Status

## Demonstrated

- **Querying config for an arbitrary key** — `config_get` walks dotted paths via facet reflection
- **Writing with FromStr parsing** — `config_set` dispatches through `shape.vtable.parse`
- **add/remove for TemplatesSets** — `AppendRemoveItem` trait + type-erased registry keyed by `ConstTypeId`
- **Default configuration values** — `DefaultsRegistry` with `Value`, `Function`, `FromKey`, `FromOptionalKey`; recursive resolution with cycle detection
- **Environment variable overrides** — multi-prefix support (`TEDGE_MAPPER_` for all keys, `TEDGE_` for cloud keys only), underscore ambiguity resolution
- **Errors for invalid/unset keys** — `ConfigError::UnknownKey`, user-facing messages with the key name
- **Schema errors point at the schema** — invalid `define_config!` input fails to compile with spans on the offending tokens: a `from_key_via` function with the wrong return type is reported against the `function` attribute, an unsupported field type against the type, and unknown attributes against the attribute (with did-you-mean suggestions). Pinned by trybuild snapshots in `config-macro/tests/compile_fail/`
- **`from_root` references validated when configs are attached** — `FederatedConfig::mount` checks every `from_root` reference in a source's schema against the root config's keys, so a misspelled root key is a mount-time `ConfigError::UnknownRootKey` naming the full user-facing key (like `DefaultsRegistry::new` already does for `from_key`) instead of a value that silently reads as unset. A `from_root` user cannot be mounted before the root config, the root config itself cannot use `from_root`, and reading a `from_root` key without a root config supplied (`RootResolver` is now fallible) is `ConfigError::NoRootConfig` rather than `None`. On the typed path, `mapper_config::load` requires the root config as a parameter and validates references before building the reader. Covered by `config-macro/tests/from_root.rs` with deliberately invalid test schemas
- **Defaults derived from another key's value** — `DefaultSpec::FromKeyVia { key, function }` (`default(from_key_via(key = "...", function = "..."))`) resolves the source key with defaults applied (including `from_root` chains) and pipes it through a fallible function. Used for `device.id` defaulting to the certificate common name in both the root and mapper schemas. Lazy (runs only when the key is read), memoized per (key, source value) for the registry's lifetime, and failures surface as `ConfigError::DerivedValue` naming the key, source, and reason; `Ok(None)` (e.g. certificate not created yet) leaves the key unset. The function returns the field's own type — `fn(&str) -> Result<Option<FieldType>, String>` — enforced at compile time by a macro-generated adapter that stringifies via `Display` for the engine, with type mismatches reported against the attribute (see `config-macro/tests/derived_defaults.rs` for a `u16` example on a test-specific schema)

## To investigate

Priority order reflects value of proving feasibility.

### ~~1. Easy struct access for code consumers~~ DONE

Reader structs with concrete types (no Options on required fields) are populated generically by `build_reader`, which walks the reader's facet shape to decide required vs optional. Callers get `config.mqtt.port` as `u16`, `config.c8y.url` as `Option<HostPort<443>>`, etc.

### ~~2. Profiled mapper configurations~~ DONE

Profiles are directory-based: a profiled mapper is a mapper named `c8y.staging` with its own `mappers/c8y.staging/mapper.toml`, mounted under `mappers.c8y.staging.*`. The CLI `--profile` flag rewrites bare cloud keys (`c8y.url` → `mappers.c8y.staging.url`), and profile environment variables (`TEDGE_C8Y_PROFILES_STAGING_URL`) are applied by `EnvOverrides::apply_for_cloud`. The generic `overlay_dto<Base, Overlay>` (merging non-None overlay fields onto a base DTO by field name) remains available in the runtime for overlay-style resolution if a future design wants it.

### ~~3. Doc comments, examples, and tab completion~~ DONE

`list_key_entries` returns `KeyEntry { key, doc }` where `doc` is `field.doc` from facet's `///` comment reflection. `cmd_list` appends `# doc` to each line. `cmd_completions` outputs `key\tdoc` for shell completion integration. All DTO leaf fields have doc comments; a test asserts this.

### ~~4. Read-only configurations~~ DONE

`ReadOnlyKeys` is a `HashSet<&'static str>` checked before `config_set`/`config_unset`/`config_add`/`config_remove`. Returns `ConfigError::ReadOnly` with the key name. The set is populated in macro output alongside the defaults registry.

### ~~5. Deprecated/renamed keys~~ DONE

`KeyAliases` maps old key paths to new ones. `resolve()` returns `(new_key, Option<old_key>)` — the CLI layer prints a deprecation warning when `old_key` is `Some`. Applied at the CLI entry point before any config operation.

### ~~6. TOML nested group serialization~~ DONE

Explicit tests confirm `skip_serializing_if = "Option::is_none"` on intermediate groups works correctly: setting `c8y.proxy.bind.port` only serializes `[c8y.proxy.bind]`, not `[c8y.proxy.client]` or `[mqtt]`. Round-trip preserves values and doesn't create spurious groups.

### 7. Guarding `cloud_type` changes

Setting `mappers.<name>.cloud_type` rewrites the mapper's file under the new schema: keys shared with the new schema are preserved, everything else is deleted (see the `cloud_type_conversion` integration tests). This means accidentally converting a well-tuned c8y config to a custom mapper config silently discards all c8y-specific settings — converting back does not restore them. A protection mechanism (e.g. requiring confirmation or a `--force` flag when the conversion would delete keys that are explicitly set) is still to be designed.

## Out of scope

- **Multi-file config** — we want this, but it's a separate investigation not tied to this PoC. Detecting cloud key access and rerouting to the mapper config implementation should be straightforward. For cross-config defaults (e.g. mapper's `c8y.device.cert_path` falling back to root tedge.toml's `device.cert_path`), a `DefaultSpec::FromParentKey` variant would resolve against an optional parent DTO passed at reader construction time. Validation at init ensures `FromParentKey` entries require a parent to be provided. The safety guarantee shifts from compile-time (macro) to runtime (registry validation at startup), which is equivalent in practice as long as both tests and prod use the same `build_reader` entrypoint — and we add a unit test that constructs the registry.
- **Post-parse validation** — not a config layer concern
- **Secrets/sensitivity** — config stores paths to key/password files, not secrets themselves
