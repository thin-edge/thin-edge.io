## Context

This design covers wiring the `tedge_config_engine` into the `tedge config` CLI
so that `mappers.*` keys work (see [proposal](proposal.md) for motivation).
The engine crate already provides `FederatedConfig`, `TypedConfigOps<T>`,
and `ConfigManager` with `from_root` resolution.
The constraint is that existing `tedge config` behaviour must remain untouched —
non-`mappers.*` keys continue through the current `ReadableKey`/`WritableKey` path.

## Scope

This PR covers **custom/user-defined mappers only** —
the ones discovered as subdirectories under `{config_dir}/mappers/`.
Built-in cloud configs (`c8y`, `aws`, `az`) remain in the existing
`define_tedge_config!` schema, accessed through `ReadableKey`/`WritableKey`.
They are not mounted in `FederatedConfig` until the full schema migration (PR 4+).

## Goals / Non-Goals

**Goals:**

- `tedge config get/set/unset/add/remove` works for `mappers.<name>.<key>` keys
  (custom mappers only).
- `tedge config list` includes keys from all discovered custom mapper configs.
- Mapper `device.*` keys fall back to root `tedge.toml` via `from_root`.
- Environment variable overrides per mapper.
- All behind a cargo feature flag; default off.

**Not in this PR** (later PRs in the migration):

- Full `tedge.toml` schema in the new macro (PR 4).
- Mapper runtime consuming config through the new engine (PR 6).

**Not in this feature:**

- Cloud profile support in the `mappers.*` key space.
  Profiles are a built-in cloud concept;
  custom mappers use separate directories instead.

**Not planned:**

- Replacing the existing `tedge mapper config get` command.
  That command resolves values with source annotations (cert CN inference, file-path provenance)
  and serves a different purpose than `tedge config get`.

## Decisions

### New crate `tedge_mapper_config`

Location: `crates/common/tedge_mapper_config/`.

Contains three things:

1. **Mapper schema** via `define_config!`, mirroring the fields in `CustomMapperConfig`:
   `url`, `device.{id, cert_path, key_path, root_cert_path}`,
   `bridge.{clean_session, keepalive_interval, tls, max_payload_size}`,
   `auth_method`, `credentials_path`.
   The `device.cert_path`, `device.key_path`, and `device.root_cert_path` fields
   use `#[tedge_config(default(from_root = "device.cert_path"))]` (and so on)
   so they inherit from the root config when unset.

2. **Root config stub** via `define_config!`, exposing only the three keys
   that mapper configs reference via `from_root`:
   `device.cert_path`, `device.key_path`, `device.root_cert_path`,
   each with the same defaults as the real `tedge.toml` schema.
   This stub is temporary — PR 4 replaces it with the full schema.
   The stub reads from `tedge.toml` on disk so its values stay in sync.

3. **`build_federated_config`** function that assembles a `FederatedConfig`:
   mounts the root stub at `""`, scans `{config_dir}/mappers/` for subdirectories,
   and mounts a `TypedConfigOps<MapperDto>` at `mappers.<name>.` for each one
   that contains a `mapper.toml`.

**Why a separate crate:**
the mapper schema depends on `tedge_config_engine` (for the macro and runtime),
and the `tedge` binary needs to use it.
Putting it in `tedge_config` would create a circular dependency
since `tedge_config` doesn't depend on the engine.
Putting it in `tedge` itself would prevent reuse by other crates later.

**Alternative considered:** define the schema inline in `tedge`.
Rejected because the mapper schema will also be needed
when the mapper runtime switches to the new engine (PR 6).

### CLI routing: wrapper enum over `ReadableKey`/`WritableKey`

A wrapper enum keeps routing in one place
and preserves clap errors for invalid core keys:

```rust
enum FederatedReadableKey {
    RootConfig(ReadableKey),
    #[cfg(feature = "mapper-config")]
    Mapper(String),
}
```

(Analogous `FederatedWritableKey`.)

`FromStr` tries `ReadableKey::from_str` first,
falls back to `Mapper(String)` for `mappers.*` keys,
and returns the standard clap error otherwise.

Shell completions are unaffected —
they use `ArgValueCandidates` (dynamic, not `ValueEnum`).
`FederatedReadableKey::completions()` combines `ReadableKey::completions()`
with mapper keys discovered from disk,
following the existing `mapper_config_key_completions()` pattern.

### Lazy construction of `FederatedConfig`

`FederatedConfig` is only built when a `mappers.*` key is encountered
or during `tedge config list`.
This avoids scanning the mappers directory
and loading TOML files for commands that don't touch mapper config.

Construction happens in a shared helper function
called from each command handler's `mappers.*` branch.
The helper takes `&TEdgeConfigLocation` (already available in every handler)
to locate the config directory.

### Environment variable overrides

Each mounted mapper source calls `apply_env_with_prefix`
with `TEDGE_CONFIG_MAPPERS_{NAME}_` (name uppercased, hyphens become underscores).

The root stub calls `apply_env_excluding` with `["mappers."]`
to avoid processing mapper-namespaced env vars.

### Feature flag: `mapper-config`

Added to `crates/core/tedge/Cargo.toml` as a non-default feature.
It gates the `tedge_mapper_config` dependency and the routing branches in each handler.
When the feature is off, `mappers.*` keys produce the existing "unknown key" error.

**Why default off:**
this is the first externally visible piece of the config engine migration.
Keeping it behind a flag lets us merge incrementally
without committing to the UX before the full migration is complete.

### Mapper schema types

`HostPort`, `SecondsOrHumanTime`, and the path types (`Utf8PathBuf`)
are defined in `tedge_config` (`crates/common/tedge_config/src/tedge_toml/models/`).
`tedge_mapper_config` depends on `tedge_config` to reuse them.
This is the same direction as the existing dependency graph
(`tedge` already depends on `tedge_config`),
and `tedge_config` does not depend on `tedge_config_engine`,
so there is no cycle.

For `AuthMethod`: the mapper crate already defines a standalone `AuthMethodConfig`
(`crates/core/tedge_mapper/src/custom/config.rs`)
with variants `Auto`, `Certificate`, `Password` and no dependencies beyond serde.
`tedge_mapper_config` defines its own copy with the same variants
to avoid depending on `tedge_mapper`.
A test asserts the two enums accept the same string values.

## Risks / Trade-offs

**Root stub drift** → The stub must match the real `tedge.toml` defaults
for `device.cert_path`, `device.key_path`, and `device.root_cert_path`.
If the real defaults change and the stub isn't updated,
mapper `from_root` resolution will produce wrong values.
Mitigation: a test in `tedge_mapper_config` that asserts the stub defaults
match the values from the real `TEdgeConfigReader`.
PR 4 eliminates the stub entirely.

**Schema drift from `CustomMapperConfig`** → The new `define_config!` schema
is a parallel definition of the mapper config.
If fields are added to `CustomMapperConfig` but not the new schema,
`tedge config` won't see them.
Mitigation: a test that compares the key sets of both schemas.
PR 6 consolidates to a single schema.

**Compile-time to runtime validation shift** → Default values and `from_root` references
are validated at runtime rather than compile time (see design decision 0012).
Mitigation: the `define_config!` macro generates tests
that validate defaults and `from_root` references.
These run in CI and catch errors before merge.
