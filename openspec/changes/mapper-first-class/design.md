## Context

This change revises D1 and D2 from `generic-mapper` and introduces two new decisions. Generic-mapper D4 (runtime wiring of bridge + flows) is unchanged. D3 (config schema) gains a new field and a file rename. D5 (service identity) and D6 (directory scanner) require updates.

---

## Revised D1: Directory convention (open question)

Both options below supersede generic-mapper D1 (`custom.{name}/` profile convention). The open question is whether to use a filesystem prefix at all.

### Preferred: no prefix

A directory under `/etc/tedge/mappers/` is a mapper if and only if it contains a `mapper.toml` file. No prefix distinguishes built-in from user-defined.

```
/etc/tedge/mappers/
  c8y/            ← pre-installed (mapper.toml backed by TEdgeConfig)
    mapper.toml
    bridge/
    flows/
  az/             ← pre-installed
    mapper.toml
  thingsboard/    ← user-created
    mapper.toml
    bridge/
    flows/
  production/     ← user-created, cloud_type = "c8y"
    mapper.toml
```

The directory scanner classifies mappers by reading `mapper.toml` (specifically `cloud_type`). Directories without `mapper.toml` are unrecognised and generate a warning.

**Why preferred**: user-defined and built-in mappers are structurally identical on disk. This is the most direct expression of the "a mapper is a mapper" goal. Users running `ls /etc/tedge/mappers/` see all mapper instances on equal footing.

**Trade-off**: the scanner must read file contents rather than classifying by name. Acceptable since it only needs to check for `mapper.toml` existence (not parse it) to detect unrecognised directories.

**Name collision**: a user cannot accidentally "shadow" a built-in mapper by creating a conflicting directory — the built-in directories (`c8y/`, `az/`, `aws/`) are pre-installed. Attempting to create a mapper whose directory already exists is a standard filesystem error. There is no concept of "reserved names" to enforce.

### Alternative: `+` prefix for user-defined mappers

User-defined mappers live in `+{name}/`; built-in mapper directories remain plain.

```
/etc/tedge/mappers/
  c8y/            ← pre-installed
  az/             ← pre-installed
  +thingsboard/   ← user-created
  +production/    ← user-created
```

The directory scanner classifies by name pattern (no file reading needed). User-defined directories are visually distinct when browsing the filesystem.

**Trade-off**: user-defined mappers are visually marked as different even when `cloud_type = "c8y"` makes them behaviourally equivalent to a built-in. This works against the first-class goal.

---

## Revised D2: CLI via clap `external_subcommand`

User-defined mappers are invoked as `tedge-mapper thingsboard`. This is implemented using clap's `#[clap(external_subcommand)]` attribute, which captures any subcommand name that does not match a built-in variant:

```rust
#[derive(Parser)]
pub enum MapperOpt {
    C8y(C8yOpt),
    Az(AzOpt),
    Aws(AwsOpt),
    Collectd(CollectdOpt),
    Flows(FlowsOpt),
    Local(LocalOpt),

    #[clap(external_subcommand)]
    UserDefined(Vec<String>),
}
```

The `Vec<String>`'s first element is the mapper name. Extra elements after the name are rejected with a clear error. Global flags (e.g. `--config-dir`) must appear before the mapper name.

At startup, the mapper name is validated:
1. Matches `[a-z][a-z0-9-]*` — otherwise hard error (required for unambiguous env var mapping; see D4)
2. The mapper directory exists and contains `mapper.toml` — otherwise error listing available user-defined mappers

**Limitations**: user-defined mapper names do not appear in `tedge-mapper --help` or tab-completion in the initial implementation. Both can be addressed later by scanning the mapper directory before printing help.

Note: built-in mapper variants (`C8y`, `Az`, `Aws`, etc.) remain as compile-time enum variants for now. The `external_subcommand` branch only handles names that don't match a built-in variant. Full migration to `external_subcommand` for all mappers (driven entirely by `mapper.toml` content) is a follow-on change.

---

## Updated D3: Config schema additions

Generic-mapper D3 (config separate from `define_tedge_config!`, `${mapper.*}` namespace) is unchanged. Two additions:

**File rename**: the config file inside mapper directories is renamed from `tedge.toml` to `mapper.toml` for all mappers — user-defined and built-in. For built-in mappers, `mapper.toml` remains the `TEdgeConfig` backing file; `tedge config set c8y.url` continues to write to `c8y/mapper.toml`.

**`cloud_type` field**: a new optional top-level field identifies which built-in cloud integration the mapper instance uses:

```toml
# Identifies the built-in cloud integration for this mapper instance.
# Absent for pure flows/bridge mappers (no cloud-specific logic).
# Valid values: "c8y", "az", "aws" — unknown values are rejected with an error.
# Note: dispatch based on this field is not yet implemented. This field
# is defined now so that tooling (tedge mapper list, tedge mapper config get)
# can report cloud type and future tooling can use it for schema selection.
cloud_type = "c8y"
```

`cloud_type` is modelled as a `CloudType` enum in both `tedge_config` (for built-in mappers) and `custom/config.rs` (for user-defined mappers). Unknown values produce a hard error at parse time — this makes the config easier to reason about and catches typos early.

When `cloud_type` is absent, the mapper runs as a pure flows/bridge mapper (existing generic-mapper behaviour). Actual dispatch — running c8y/az/aws logic for a user-defined mapper instance — is deferred to a follow-on change.

For built-in mapper directories, `cloud_type` is written automatically by `migrate_mapper_config` (alongside `mapper_config_dir`). It is stored as a `#[tedge_config(reader(skip))]` field in the c8y/az/aws DTO sections, so it is serialised to `mapper.toml` but is not user-configurable via `tedge config set`.

---

## New D3 (this change): `tedge mapper` CLI

A new `Mapper` subcommand is added to `TEdgeOpt` in the `tedge` binary:

```
tedge mapper list
tedge mapper config get <name>.<key>
```

**`tedge mapper list`** scans `/etc/tedge/mappers/` and prints all mapper directories — those with `mapper.toml` — with their `cloud_type` if set:

```
c8y           cloud_type=c8y
az            cloud_type=az
thingsboard   (no cloud_type)
production    cloud_type=c8y
```

**`tedge mapper config get <name>.<key>`** reads a value from the named mapper's `mapper.toml`. The argument is split on the first `.`: the leading segment is the mapper name, the remainder is the TOML key path. Since mapper names match `[a-z][a-z0-9-]*` (no `.`), this split is unambiguous.

```
tedge mapper config get thingsboard.url
tedge mapper config get thingsboard.device.cert_path
tedge mapper config get production.bridge.topic_prefix
```

Output is the raw value, matching `tedge config get` behaviour. Errors clearly if the mapper directory doesn't exist, `mapper.toml` is absent, or the key path is not found.

**Why `tedge mapper`, not `tedge config`**: `tedge config` is coupled to `define_tedge_config!` and its compile-time schema. Mapper config is intentionally outside that schema (D3). A dedicated `tedge mapper` subcommand can grow independently (e.g. `tedge mapper init`, `tedge mapper config set`) without coupling to the global config machinery.

---

## New D4 (this change): Environment variable override scheme (design only — implementation deferred)

Mapper config keys can be overridden via environment variables of the form `MAPPER_{NAME}_{KEY}`, where:
- `{NAME}` is the mapper name uppercased with hyphens replaced by underscores
- `{KEY}` is the TOML key path uppercased with dots replaced by underscores

| `mapper.toml` key | Mapper | Env var |
|---|---|---|
| `url` | `thingsboard` | `MAPPER_THINGSBOARD_URL` |
| `device.cert_path` | `thingsboard` | `MAPPER_THINGSBOARD_DEVICE_CERT_PATH` |
| `bridge.topic_prefix` | `my-cloud` | `MAPPER_MY_CLOUD_BRIDGE_TOPIC_PREFIX` |

**Why underscores are forbidden in mapper names**: `+my_cloud` and `+my-cloud` would both map to `MAPPER_MY_CLOUD_*`, making env var targeting ambiguous. Forbidding underscores (hard error at startup) makes the mapping bijective. This is enforced in D2 validation.

---

## Updated D5: Service identity

For `tedge-mapper thingsboard`:

| | Value |
|---|---|
| Service name | `tedge-mapper@thingsboard` |
| Health topic | `te/device/main/service/tedge-mapper@thingsboard/status/health` |
| Lock file | `/run/tedge-mapper@thingsboard.lock` |
| Bridge service name | `tedge-mapper-bridge@thingsboard` |

This replaces `tedge-mapper-custom@{profile}` from generic-mapper D5.

---

## Updated D6: Directory scanner

Under the **no-prefix** approach, the scanner classifies by `mapper.toml` presence:

| Directory contents | Classification |
|---|---|
| Contains `mapper.toml` | mapper (inspect `cloud_type` for type) |
| No `mapper.toml` | unrecognised → warning |

Under the **`+` prefix** approach, the scanner classifies by name pattern (no file reading):

| Directory pattern | Classification |
|---|---|
| `c8y`, `az`, `aws`, `collectd`, `flows`, `local` | built-in |
| `{builtin}.{profile}` | profile of built-in |
| `+{name}` | user-defined |
| Anything else | unrecognised → warning |

The correct implementation depends on the D1 decision.
