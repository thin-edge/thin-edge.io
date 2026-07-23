# Facet Config Prototype

A proof-of-concept replacement for thin-edge.io's configuration system, built on the [facet](https://github.com/facet-rs/facet) reflection library.

## What is facet?

Facet is a Rust reflection library that lets you inspect struct shapes at runtime — field names, types, doc comments, and vtables — without writing manual codegen for each operation. You derive `#[derive(Facet)]` and the library gives you a `Shape` you can walk programmatically.

This matters because the current tedge config macro has to generate bespoke code for every operation (get, set, list, read, overlay, ...) by emitting match arms for every key path at compile time. Facet replaces all of that with a single generic reflection walk.

## What does the prototype cover?

All the features of the existing `tedge config` CLI are demonstrated:

| Feature | Status |
|---|---|
| `get` / `set` / `unset` with dotted key paths | Done |
| `add` / `remove` for list-valued keys (TemplatesSet) | Done |
| Default values (literal, function, derived from other keys) | Done |
| Environment variable overrides with `TEDGE_` prefix | Done |
| Typed reader structs (non-Optional for keys with defaults) | Done |
| Deprecated/renamed key aliases with warnings | Done |
| Read-only keys | Done |
| Doc comments and examples on `list` / `completions` | Done |
| `HostPort<const P: u16>` with default-port const generic | Done |
| TOML nested group serialization (no spurious empty groups) | Done |
| Federated config (root tedge.toml + per-mapper configs) | Done |
| Cloud profiles (`--profile prod` for `c8y.prod`) | Done |
| Cross-config defaults (mapper cert_path falls back to root) | Done |

## Federated config

thin-edge.io already has separate config files — `tedge.toml` for the core and per-mapper TOML files. The core config has full CLI support (get/set/unset/add/remove) via `tedge config`, while the mapper config only supports reading keys via `tedge mapper config get`. The two are powered by completely separate implementations with no shared machinery.

This prototype unifies them under one config engine so both get the full feature set — get, set, unset, add, remove, defaults, env var overrides, typed readers — from the same codebase. Each file is mounted under a key prefix:

```
/etc/tedge/
  tedge.toml                    # root: device, mqtt
  mappers/
    c8y/mapper.toml             # c8y-specific config
    c8y.prod/mapper.toml        # c8y with profile "prod"
    az/mapper.toml              # azure mapper config
    custom-foo/mapper.toml      # user-defined mapper
```

`FederatedConfig` routes reads and writes to the correct file based on the key prefix. Each mapper can have its own DTO type — c8y has extra fields like `smartrest.templates` and `availability.interval` that a generic mapper doesn't — but they all get the same CLI experience.

Cross-config defaults also work: a mapper's `device.cert_path` can fall back to the root `tedge.toml`'s `device.cert_path` via `default(from_root = "device.cert_path")`.

The CLI resolves `c8y.url` to `mappers.c8y.url` via a backward-compat alias layer, so existing user muscle memory works unchanged.

## Environment variables: reads versus writes

Environment variables are read-time overrides. The read-only `get`, `list`,
and `show` commands display the effective configuration after TOML, defaults,
and environment variables have been composed. This makes `get` useful for
diagnosing the configuration seen by a running process:

```bash
TEDGE_MQTT_PORT=8883 cargo run -- --config-dir ./test-config get mqtt.port
# 8883
```

Persistent commands have deliberately different semantics. `set`, `unset`,
`add`, and `remove` reload the TOML file without environment overrides, apply
the requested change, and write that result. An environment value is therefore
never copied into TOML and does not become the starting value for `add` or
`remove`. In the example below, only `device.type` is changed in the file;
`TEDGE_MQTT_PORT` remains an environment-only override:

```bash
TEDGE_MQTT_PORT=8883 cargo run -- --config-dir ./test-config set device.type custom
```

## How the macro works

A config is defined with a lightweight DSL:

```rust
facet_config_macro::define_config! {
    TEdge {
        device: {
            /// Unique device identifier
            #[tedge_config(example = "my-device-001")]
            id: String,

            /// Device type identifier
            #[tedge_config(rename = "type", default(value = "thin-edge.io"))]
            ty: String,
        },
        mqtt: {
            /// MQTT broker port
            #[tedge_config(default(value = "1883"))]
            port: u16,
        },
    }
}
```

The macro generates:

1. **DTO struct** (`TEdgeConfigDto`) — all fields `Option<T>`, derives `Facet + Serialize + Deserialize`. This is what gets read from / written to TOML.
2. **Reader struct** (`TEdgeConfig`) — fields with defaults are concrete types (e.g. `port: u16`), fields without defaults are `Option<T>`. Built generically from the DTO + defaults registry.
3. **Registry functions** — `build_defaults()`, `build_registry()`, `build_read_only_keys()`, `build_aliases()`, `build_examples()`.

The key insight: the macro only generates **data declarations and registrations**. All the actual logic (walking keys, getting/setting values, overlaying DTOs, building readers) lives in `config-runtime` as generic functions that use facet reflection. The macro never emits match arms or per-key logic.

## Comparison with the existing macro

| | Existing (`tedge_config_macros`) | Prototype (`facet-config-macro`) |
|---|---|---|
| Macro impl | ~4,400 lines | ~1,450 lines |
| Runtime library | ~300 lines (mostly in macro output) | ~2,100 lines |
| **Total** | **~4,700 lines** | **~3,550 lines** |
| Approach | Generates per-key match arms for every operation | Generates structs + registries; operations are generic |
| `query.rs` (key lookup) | 1,841 lines of codegen | 0 — replaced by `reflect.rs` (643 lines, generic) |
| `reader.rs` (typed reader) | 1,253 lines of codegen | 250 lines macro + 105 lines generic `build_reader` |
| Adding a new operation | Add a new codegen pass in the macro | Write a generic function over `Shape` |

The macro is simpler because facet gives it reflection for free — field traversal, type-erased parsing via vtables, doc comment access — so the macro doesn't need to reinvent any of that.

## Crate structure

```
crates/
  config-macro/          # Thin proc-macro wrapper (29 lines)
  config-macro-impl/     # Macro implementation: parse DSL, emit structs + registries
  config-runtime/        # Generic reflection-based operations (get/set/list/read/overlay/defaults)
  tedge-config/          # Root config definition (device, mqtt)
  mapper-config/         # Mapper config definition (generic + c8y-specific)
```

## Running it

```bash
cargo run -- --config-dir ./test-config get mqtt.port
cargo run -- --config-dir ./test-config set c8y.url mytenant.cumulocity.com
cargo run -- --config-dir ./test-config list
cargo run -- --config-dir ./test-config --profile prod get c8y.url
```

Tests:
```bash
cargo test          # unit + integration tests
cargo test cli      # CLI integration tests only
```
