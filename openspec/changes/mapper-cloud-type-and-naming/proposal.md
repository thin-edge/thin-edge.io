## Why

`tedge config` can read and write `mappers.*` keys since #4328,
but it is not yet safe to point at a real `mapper.toml`.
Writes silently delete keys the schema doesn't know, a key custom mappers already use is missing,
profiled mappers ignore the environment,
and a profile key can be mistaken for a key of the base mapper.
These need fixing before the c8y schema lands, because that schema depends on all four.

## Proposed solution

| Command | Today | After |
|---|---|---|
| `tedge config set mappers.c8y.url …` on a file with `[smartrest]` | rewrites `mapper.toml`, deleting `[smartrest]` | error naming `smartrest.templates`; file unchanged |
| `tedge config get mappers.thingsboard.cloud_type` | unknown key | returns the value |
| `tedge config set mappers.c8y.profiles.prod.url …` | unknown key (profile keys are `mappers.c8y.prod.url`) | writes `url` in `mappers/c8y.prod/` |
| `tedge config set mappers.c8y.bridge.url …` | creates `mappers/c8y.bridge/` | unknown key `bridge.url` of `c8y` |
| `TEDGE_MAPPERS_C8Y_PROFILES_PROD_URL=…` | ignored | overrides `url` in `mappers/c8y.prod/` |

`cloud_type` is added as a key only.
Every mapper keeps the generic schema whatever its `cloud_type`,
so setting it never adds or removes other keys.
Choosing a schema from `cloud_type` arrives with the c8y schema.

## What Changes

- A write to a `mapper.toml` that has keys outside the schema fails with an error naming them, and the file is unchanged.
- New key `mappers.<name>.cloud_type`: `c8y`, `az` or `aws`.
- Keys of a built-in mapper profile move from `mappers.<cloud>.<profile>.<key>` to `mappers.<cloud>.profiles.<profile>.<key>`,
  matching the legacy `<cloud>.profiles.<profile>.<key>`.
  The directory stays `mappers/<cloud>.<profile>/`.
  **BREAKING** for the `mapper-config` feature only, which no release enables.
- Mapper environment variable names follow the key: `TEDGE_MAPPERS_C8Y_PROFILES_PROD_URL`.
  `.` and `-` in a directory name become `_`.
  If two mapper directories produce the same name (`thingsboard.eu`, `thingsboard-eu`),
  every read and write of either fails with an error naming both; the other mappers keep working.
- `tedge mapper config get c8y.prod.url` keeps working, reading `mappers.c8y.profiles.prod.url`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `config-engine-cli-routing`: the four behaviours above.

## Impact

- `tedge_config_engine`: `TypedConfigOps` checks the file for unknown keys before writing.
- `tedge_mapper_config`: `cloud_type` and its `CloudType` model, the `profiles` mount prefix, env prefix rules.
- `tedge` CLI: directory creation and `tedge mapper config get`, which translates the old key names, use the new prefix.
- The mapper schema's key-set test moves to `tedge_mapper`, comparing against `custom_mapper_schema_keys()` instead of a hardcoded list.
- Everything is behind the `mapper-config` feature, which is off by default; no released behaviour changes.
