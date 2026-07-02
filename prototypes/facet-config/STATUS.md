# Facet Config Prototype Status

## Demonstrated

- **Querying config for an arbitrary key** — `config_get` walks dotted paths via facet reflection
- **Writing with FromStr parsing** — `config_set` dispatches through `shape.vtable.parse`
- **add/remove for TemplatesSets** — `AppendRemoveItem` trait + type-erased registry keyed by `ConstTypeId`
- **Default configuration values** — `DefaultsRegistry` with `Value`, `Function`, `FromKey`, `FromOptionalKey`; recursive resolution with cycle detection
- **Environment variable overrides** — multi-prefix support (`TEDGE_MAPPER_` for all keys, `TEDGE_` for cloud keys only), underscore ambiguity resolution
- **Errors for invalid/unset keys** — `ConfigError::UnknownKey`, user-facing messages with the key name

## To investigate

Priority order reflects value of proving feasibility.

### ~~1. Easy struct access for code consumers~~ DONE

Reader structs with concrete types (no Options on required fields) are populated generically by `build_reader`, which walks the reader's facet shape to decide required vs optional. Callers get `config.mqtt.port` as `u16`, `config.c8y.url` as `Option<HostPort<443>>`, etc.

### ~~2. Profiled mapper configurations~~ DONE

Profiles stored as `HashMap<String, C8yProfileDto>` inside `C8yConfigDto`, deserialized naturally from TOML (`[c8y.profiles.production]`). Profile resolution uses generic `overlay_dto<Base, Overlay>` which walks the overlay's fields and overwrites matching fields in a clone of the base — works across different types as long as field names match. The merged DTO feeds straight into `build_reader`. `list_keys` skips Map fields (profiles don't appear as settable keys). Env var interaction for profiles (resolving `TEDGE_C8Y_PROD_URL`) is sketched but not yet implemented.

### ~~3. Doc comments, examples, and tab completion~~ DONE

`list_key_entries` returns `KeyEntry { key, doc }` where `doc` is `field.doc` from facet's `///` comment reflection. `cmd_list` appends `# doc` to each line. `cmd_completions` outputs `key\tdoc` for shell completion integration. All DTO leaf fields have doc comments; a test asserts this.

### ~~4. Read-only configurations~~ DONE

`ReadOnlyKeys` is a `HashSet<&'static str>` checked before `config_set`/`config_unset`/`config_add`/`config_remove`. Returns `ConfigError::ReadOnly` with the key name. The set is populated in macro output alongside the defaults registry.

### ~~5. Deprecated/renamed keys~~ DONE

`KeyAliases` maps old key paths to new ones. `resolve()` returns `(new_key, Option<old_key>)` — the CLI layer prints a deprecation warning when `old_key` is `Some`. Applied at the CLI entry point before any config operation.

### ~~6. TOML nested group serialization~~ DONE

Explicit tests confirm `skip_serializing_if = "Option::is_none"` on intermediate groups works correctly: setting `c8y.proxy.bind.port` only serializes `[c8y.proxy.bind]`, not `[c8y.proxy.client]` or `[mqtt]`. Round-trip preserves values and doesn't create spurious groups.

## Out of scope

- **Multi-file config** — we want this, but it's a separate investigation not tied to this PoC. Detecting cloud key access and rerouting to the mapper config implementation should be straightforward. For cross-config defaults (e.g. mapper's `c8y.device.cert_path` falling back to root tedge.toml's `device.cert_path`), a `DefaultSpec::FromParentKey` variant would resolve against an optional parent DTO passed at reader construction time. Validation at init ensures `FromParentKey` entries require a parent to be provided. The safety guarantee shifts from compile-time (macro) to runtime (registry validation at startup), which is equivalent in practice as long as both tests and prod use the same `build_reader` entrypoint — and we add a unit test that constructs the registry.
- **Post-parse validation** — not a config layer concern
- **Secrets/sensitivity** — config stores paths to key/password files, not secrets themselves
