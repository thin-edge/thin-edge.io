## Goal

Enable users to create cloud mappers without writing Rust — only TOML config and flow scripts — and have them be first-class citizens alongside built-in mappers. A mapper is a directory under `/etc/tedge/mappers/`; it is invoked as `tedge-mapper <name>` and managed like any other service. The directory alone is sufficient — `mapper.toml` is only needed when establishing a cloud bridge.

## Decisions

### Directory convention: no prefix, directory existence is sufficient

Any directory under `/etc/tedge/mappers/` is a mapper. No `mapper.toml` is required — an empty directory is valid and starts in flows-only mode. User-defined and built-in directories are structurally identical on disk (`thingsboard/` alongside `c8y/`). No prefix (`custom.`, `+`, etc.) is needed.

On startup, the mapper creates the `flows/` subdirectory if it does not exist, so flow scripts can be dropped in at runtime without restarting. Separately, when any mapper process starts it scans all directories under `/etc/tedge/mappers/` and warns about any that contain `bridge/` without `mapper.toml` — those will always fail to start and are likely a configuration mistake. Empty and flows-only directories are valid and do not warn.

### CLI: `external_subcommand` for user-defined mappers

```rust
#[clap(external_subcommand)]
UserDefined(Vec<String>),
```

Any subcommand not matching a built-in variant is treated as a mapper name. Extra arguments after the name are rejected. Names must match `[a-z][a-z0-9-]*` (required for unambiguous env var mapping; underscores are rejected with a hard error). Names starting with `bridge-` are also rejected — they would collide with the `tedge-mapper-bridge-{name}` sub-service pattern.

Built-in variants (`C8y`, `Az`, etc.) remain as compile-time enum variants for now. Full migration to content-driven dispatch is a follow-on.

### Config lives outside `define_tedge_config!`; bridge templates gain `${mapper.*}`

`mapper.toml` in the mapper directory is parsed directly by the custom mapper and is invisible to `TEdgeConfig`. It is optional for flows-only mappers (only required when `bridge/` is present). The bridge template system gains a `${mapper.*}` namespace resolved against the mapper's own TOML table — implemented by adding `mapper_config: Option<&toml::Table>` to `TemplateContext`.

`define_tedge_config!` is a compile-time macro; custom mappers are a runtime concept with dynamic names. Fitting them in would require `#[tedge_config(multi)]` with dynamic key insertion for no benefit — no other component references custom mapper config.

Config file is named `mapper.toml` for all mappers (built-in and user-defined). For built-in mappers, `mapper.toml` remains the `TEdgeConfig` backing file; `tedge config set c8y.url` continues to work.

A new optional `cloud_type` field identifies which built-in cloud integration a mapper uses (`"c8y"`, `"az"`, `"aws"`). Unknown values are a hard error. Actual dispatch based on this field is deferred — it is defined now so `tedge mapper list` can report it and future tooling can use it for schema selection.

Path fields in `mapper.toml` (`device.cert_path`, `device.key_path`, `device.root_cert_path`, `credentials_path`) support relative paths, resolved relative to the mapper directory at parse time. When `device.cert_path`/`device.key_path` are absent, they fall back to the root `tedge.toml` values, so users don't need to duplicate device cert config per mapper.

### Startup validation: single typed-return function

```rust
pub enum MapperStartup {
    FlowsOnly,
    WithBridge { config: CustomMapperConfig, has_flows: bool },
}
```

`validate_and_load(mapper_dir, config_dir)` is the sole validation entry point. Sequence: directory must exist → if `bridge/` is present, `mapper.toml` must exist and be valid → otherwise start in flows-only mode (creating `flows/` if absent). Collapsing to one function with a typed return makes spec-implementation drift a compile error rather than a silent inconsistency.

### Service identity

For `tedge-mapper thingsboard`: service name `tedge-mapper-thingsboard`, health topic `te/device/main/service/tedge-mapper-thingsboard/status/health`, lock file `/run/tedge-mapper-thingsboard.lock`, bridge service name `tedge-mapper-bridge-thingsboard`.

### `tedge mapper` CLI

```
tedge mapper list
tedge mapper config get <name>.<key>
```

`list` scans all subdirectories under `/etc/tedge/mappers/` and prints name + `cloud_type` (from `mapper.toml` if present). `config get` splits on the first `.` to extract mapper name and TOML key path, then reads and walks `mapper.toml`.

A dedicated subcommand rather than extending `tedge config` — mapper config is intentionally outside the `define_tedge_config!` schema and should grow independently.

### Env var override scheme (design only — implementation deferred)

`MAPPER_{NAME}_{KEY}` where hyphens in the name map to underscores. Forbidding underscores in mapper names (enforced at startup) makes the mapping bijective.

## Non-goals / deferred

- `tedge mapper config set` — read-only CLI access is sufficient for now
- Shell completions and `--help` listing for user-defined mappers — can be added by scanning the mapper directory at help-print time
- Env var overrides — scheme is designed; implementation deferred
- Full migration of built-in mappers to content-driven dispatch via `cloud_type`

## Risks / trade-offs

**Convenience vs correctness of directory-as-registration**

Any directory under `/etc/tedge/mappers/` is a mapper — no explicit registration is needed. The trade-off is that accidental directories, misplaced files, or an existing directory intended for another purpose will silently be treated as a (flows-only) mapper. This is an accepted trade-off: the convention is intentionally minimal, the directory path is deliberate, and the `bridge/`-without-`mapper.toml` warning catches the most likely class of misconfiguration (a half-configured mapper that would fail to start anyway).
