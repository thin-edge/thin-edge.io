## Why

The `generic-mapper` change established the foundation for user-defined mappers. The long-term goal is for there to be **no distinction between a "custom" mapper and a "built-in" mapper** from the user's perspective — a mapper is a mapper. Two decisions from that work pull in the wrong direction:

- **D1 (`custom.{name}/` directories)** embeds the word "custom" into the filesystem path, permanently distinguishing user-defined mappers from built-in ones.
- **D2 (`tedge-mapper custom --profile <name>`)** frames a user-defined mapper as a _profile of something called "custom"_ rather than a first-class named entity. The `--profile` convention is also not widely adopted across thin-edge.

Both decisions were pragmatic given the constraints at the time, but they work against the first-class goal and should be corrected before users build on top of them.

Additionally, config discoverability was deferred in `generic-mapper`: users edit `mapper.toml` directly with no CLI access. The right answer is a dedicated `tedge mapper` CLI (rather than extending `tedge config`, which is coupled to the compile-time `define_tedge_config!` schema) — at minimum read-only access to mapper config is needed.

## What Changes

- **Config file name**: `tedge.toml` inside mapper directories becomes `mapper.toml` for _all_ mappers — built-in (`c8y/`, `az/`, `aws/`) and user-defined alike. For built-in mappers, `mapper.toml` continues to be backed by `TEdgeConfig`; `tedge config set c8y.url` still writes to `c8y/mapper.toml` and works as before.
- **`cloud_type` field** (schema definition only; dispatch is a follow-on change): a new optional field in `mapper.toml` identifies which built-in cloud integration a mapper instance uses. Absent for pure flows/bridge mappers. When set (e.g. `cloud_type = "c8y"`), it also determines the schema that `tedge mapper config set` (future) uses to validate and write config values.
- **Directory convention (open question — see design)**: preferred approach — no prefix; a directory under `/etc/tedge/mappers/` is a mapper iff it contains `mapper.toml`, making built-in and user-defined directories structurally identical. Alternative — user-defined mappers use a `+{name}/` prefix to distinguish them from pre-installed built-in directories at a glance.
- **CLI invocation**: `tedge-mapper thingsboard` replaces `tedge-mapper custom --profile thingsboard`; the mapper name maps directly to a directory under `/etc/tedge/mappers/`
- **Service identity**: `tedge-mapper-{name}` (e.g. `tedge-mapper-thingsboard`), matching the convention of built-in mappers and not tied to any specific init system
- **New `tedge mapper` subcommand** in the `tedge` binary:
  - `tedge mapper list` — lists all mappers with their `cloud_type` if set
  - `tedge mapper config get <name>.<key>` — reads a key from `{name}/mapper.toml` using the same dotted syntax as `tedge config get`
- **Environment variable overrides** (design only in this change; implementation deferred): mapper config keys can be overridden via `MAPPER_{NAME}_{KEY}` (e.g. `MAPPER_THINGSBOARD_URL`). Mapper names are restricted to `[a-z][a-z0-9-]*`; hyphens map to underscores in the env var name. Underscores in mapper names are rejected with a hard error at startup.

## What Stays the Same

Everything else from `generic-mapper`: the `${mapper.*}` bridge template namespace, the mapper config schema fields (TLS config, auth method — same as before, file just renamed), flows-only mapper support, bridge-without-config error behaviour, and all unimplemented tasks from that change (integration tests, documentation).

## Open Questions

### Directory naming convention

**Preferred**: no prefix — a directory is a mapper iff it contains `mapper.toml`. User-defined and built-in directories are structurally identical; `ls /etc/tedge/mappers/` shows `c8y/` and `thingsboard/` side by side. Requires reading file contents (not just names) to classify directories.

**Alternative**: `+` prefix for user-defined mappers — `+thingsboard/` alongside `c8y/`. Classification is name-based (no file reading). User-defined mappers are visually distinct but feel slightly second-class.

### Env var rollout to built-in mappers

`TEDGE_C8Y_URL` already exists for the built-in c8y mapper. For full first-class parity, `MAPPER_C8Y_URL` should eventually also work. This is left for a follow-on change but the naming scheme chosen here should not make it harder.

## Capabilities

### Modified Capabilities

- `custom-mapper-config`: directory convention (open question), config file rename (`mapper.toml`), `cloud_type` field addition, env var override scheme
- `custom-mapper-runtime`: CLI invocation (`tedge-mapper <name>`), service identity (`tedge-mapper@{name}`)

### New Capabilities

- `custom-mapper-cli`: the `tedge mapper` subcommand — listing mappers and reading config values

## Impact

- **tedge_mapper crate**: `MapperName` enum loses `Custom { profile }` variant, gains dynamic dispatch via clap `external_subcommand`; `CustomMapper` struct loses `profile` field, gains `name: String`; `mapper.toml` replaces `tedge.toml` as the config file name inside all mapper directories
- **tedge crate**: new `Mapper` variant in `TEdgeOpt` with `list` and `config get` subcommands
- **Built-in mapper directories**: `tedge.toml` renamed to `mapper.toml`; `TEdgeConfig` backing unchanged; `cloud_type` field added to distinguish mapper type
- **Risk**: achieving true first-class parity (env var overrides, `tedge mapper config set` for built-in schema) requires follow-on work; this change's design choices should not foreclose it
- **No impact** on `tedge_mqtt_bridge` or built-in mapper behaviour
