## Context

This change makes `mapper.toml` safe to write through `tedge config` (see [proposal](proposal.md)).

- `tedge_mapper_config` may only use `tedge_config` in tests, to keep the old engine out of the mapper's binary.
- The c8y, az and aws schemas come next.
  Nothing here should need changing when they add more keys.
- Everything stays behind the `mapper-config` feature.

## Goals / Non-Goals

**Goals:**

- A write never deletes a key from `mapper.toml`.
- `cloud_type` accepts the same values as in production.
- Every mapper directory can be overridden from the environment.
- Every `mappers.*` key names exactly one mapper and key, whatever groups later schemas add.

**Non-Goals:**

- Picking the schema from `cloud_type`, and stopping `cloud_type` changes from losing keys.
  Both come with the c8y schema.
- Keeping comments in `mapper.toml`.
  The old engine doesn't either.
- Keeping keys that aren't in the schema.
  If custom mappers need their own keys, they get their own group or file.
- Environment overrides for `tedge.toml` values.
  The root config has no environment prefix today, so the call that applies them does nothing.
  It is deleted here; the real root `device` table brings working overrides.

## Decisions

### Writes refuse files with unknown keys

A write reads `mapper.toml` into the typed config, changes one key, and writes the typed config back over the file.
The typed config only has the keys the schema defines, so any other key in the file is deleted.
`mapper.toml` has a defined schema, so a key outside it is an error, and the write stops before deleting anything:

```diff
 tedge config set mappers.c8y.url <value>
   read mapper.toml into the typed config
+  if mapper.toml has keys the schema doesn't define
+    error "Not writing mappers/c8y/mapper.toml, as it has keys this schema doesn't know: smartrest.templates"
   set url in the typed config
   write the typed config over mapper.toml
```

Reads are unaffected.
Until the c8y schema lands this blocks writes to a `mappers/c8y/mapper.toml` with c8y keys in it;
after that, an unknown key really is a mistake in the file.

The check compares every key in the file, read as plain TOML, with the schema's keys.
`tedge_config_engine` gains a dependency on `toml`, which is already in the workspace.

### A broken mapper file only affects that mapper

`tedge config` loads every mapper directory before running any `mappers.*` command.
Today a file that fails to parse aborts that load,
so one bad `mapper.toml` breaks `get`, `set` and `list` for every mapper,
with an error naming a file the user didn't ask about.

Instead, each mapper keeps its own load error:

```diff
 load all mapper directories
-  parse mapper.toml                  # an error aborts the whole load
+  parse mapper.toml                  # an error is kept with this mapper
 tedge config get/set mappers.<name>.<key>
+  if this mapper's file didn't parse: fail with that error
 tedge config list
+  warn once per file that didn't parse, and list every other mapper
```

Loading only the mapper a command names was the alternative.
It would fix `get` and `set` but not `list`, which reads every mapper.

### `cloud_type` uses its own copy of `CloudType`

`tedge_mapper_config` gets its own `CloudType { C8y, Az, Aws }`.
A test checks it accepts and prints the same strings as `tedge_config::models::CloudType`,
like the existing `AuthMethod` test.
The copy is deleted along with the old engine.

### The key list test moves to `tedge_mapper`

The test that checks the schema's keys compares against `custom_mapper_schema_keys()` instead of a hardcoded list.
That function lives in `tedge_mapper`, so the test does too, using `tedge_mapper_config` as a dev-dependency.

```text
assert schema keys == custom_mapper_schema_keys() + ["cloud_type"]
```

`custom_mapper_schema_keys()` doesn't list `cloud_type`, because it only lists the keys templates can use.

### Profile keys go under `profiles`

Profile `prod` is the directory `mappers/c8y.prod/`.
Today its keys are `mappers.c8y.prod.<key>`, so `tedge config` has to guess where the mapper name ends:

```text
mappers.c8y.bridge.url
  "c8y"        + "bridge.url"   not a key
  "c8y.bridge" + "url"          a key → creates mappers/c8y.bridge/
```

Once that directory exists, `mappers.c8y.bridge.tls` goes to it instead of `bridge.tls` of `c8y`.
Checking profile names on write doesn't fix this:
as soon as a later schema adds a group with the same name as a profile created today,
the profile and the group compete for the same keys.

Instead, profile keys get their own segment, matching the legacy `c8y.profiles.prod.url`:

```text
mount prefix(dir)
  "c8y.prod"  (built-in cloud + profile) → "mappers.c8y.profiles.prod."
  anything else                          → "mappers.<dir>."

mappers.c8y.bridge.tls           → mapper c8y,         key bridge.tls
mappers.c8y.profiles.bridge.url  → mapper c8y.bridge,  key url
mappers.c8y.prod.url             → mapper c8y,         key prod.url → unknown key
```

No key is ambiguous, whatever groups a schema adds later.
`profiles` can never be a group in a cloud schema; a test checks this.
The same prefix function is used to mount directories
and by `tedge mapper config get`, which turns `c8y.prod.url` into `mappers.c8y.profiles.prod.url`
when `mappers/c8y.prod/` exists.
Choosing the directory to create on write goes the other way:
`extract_builtin_mapper_name` parses `mappers.<cloud>.profiles.<profile>.` and `mappers.<cloud>.` back into a directory name.

### Environment variable names

Like every other tedge variable, the name is `TEDGE_` followed by the key,
so profile variables match the legacy `TEDGE_C8Y_PROFILES_PROD_URL`:

```text
env prefix(dir) = "TEDGE_" + uppercase(mount prefix(dir)) with "." and "-" replaced by "_"
  c8y.prod    → TEDGE_MAPPERS_C8Y_PROFILES_PROD_
  thingsboard → TEDGE_MAPPERS_THINGSBOARD_

for each mapper dir
  if env prefix(dir) is also used by another dir
    every read and write of either dir fails with
      "mappers/thingsboard.eu and mappers/thingsboard-eu both use TEDGE_MAPPERS_THINGSBOARD_EU_"
```

An error is better than picking one directory, because the variable would quietly apply to a mapper the user didn't mean.
Only the two colliding mappers fail, so the other mappers keep working, the same as with a broken `mapper.toml`.

The old engine reads every `TEDGE_*` variable as one of its own keys and warns about the ones it doesn't know,
so it skips `TEDGE_MAPPERS_*`.
Without that, every `tedge` command would warn about each mapper variable.

## Risks / Trade-offs

- [`tedge config set` can't write to `mappers/c8y/mapper.toml` until the c8y schema lands, if the file has c8y keys]
  → Only with the `mapper-config` feature on.
  The error names the keys, and the old `c8y.*` keys still write the file.
- [A key renamed in the schema, read through an alias, counts as unknown]
  → The `log_management` alias comes with the c8y schema, and must be accepted by this check.
- [Profile keys written as `mappers.c8y.prod.*` since #4328 stop working]
  → They were only available behind the `mapper-config` feature, which is off by default, so no release has them.
