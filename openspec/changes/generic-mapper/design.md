## Context

A thin-edge mapper combines an MQTT bridge (connecting local broker to cloud) with conversion logic (translating between thin-edge and cloud message formats). Built-in mappers for Cumulocity, Azure IoT, and AWS are compiled into `tedge-mapper` with hardcoded CLI subcommands, config schema, and cloud-specific converters. The Cumulocity mapper is the most complex example — it wires up the built-in bridge (with bridge rules loaded from TOML templates in its `bridge/` directory), hardcoded Rust logic for handling operations, and flows. The flows engine and built-in MQTT bridge now provide all the building blocks needed for user-defined mappers, but the wiring between CLI, config, bridge, and flows is still tied to the built-in mapper set.

The relevant existing infrastructure:

- **`MapperName` enum** (clap subcommands): `Az`, `Aws`, `C8y`, `Collectd`, `Flows` — compile-time, no extensibility for unknown names
- **`define_tedge_config!` macro**: generates typed config at compile time; supports multi-profile via `#[tedge_config(multi)]` (used for c8y, az, aws). Profile names stored in `HashMap<ProfileName, T>`. Config values are referenced throughout thin-edge, so the global schema is large
- **`populate_mapper_configs`**: discovers mapper directories on disk, reads per-mapper `tedge.toml` files, populates config DTOs. Currently hardcoded to c8y/az/aws
- **`SpecialisedCloudConfig` trait**: links a cloud type to its config DTO and reader. Each built-in mapper implements this
- **Bridge rule TOML with templating**: supports `${config.c8y.url}` (resolved against `TEdgeConfig`), `${item}` (loop variable), and `${connection.auth_method}` (connection context). The `TemplateContext` struct holds a `TEdgeConfig` reference and a loop variable value — no other namespaces exist currently
- **Flows engine**: directory-based discovery (`/etc/tedge/mappers/{name}/flows/`), hot-reloading, already works with arbitrary mapper names via `flows_dir()`

## Goals / Non-Goals

**Goals:**

- Users can create a custom mapper by creating a `custom.{name}/` directory under `/etc/tedge/mappers/` containing any combination of a `tedge.toml` (cloud connection settings), bridge rules, and flow scripts
- A custom mapper runs the built-in MQTT bridge (if `tedge.toml` is present) and flows engine (if `flows/` is present) as a single service
- Multiple custom mappers can coexist independently as separate profiles
- Built-in mappers are completely unaffected

**Non-Goals:**

- `tedge config` CLI integration for custom mapper settings (deferred — users edit their mapper's `tedge.toml` directly for now)
- Custom mappers appearing in `tedge-mapper --help` and shell completions (incremental follow-up)
- A packaging/distribution format for custom mappers
- Custom code-based converters (all conversion is via flows)

**Aspirational (may or may not be worth pursuing):**

- First-class `tedge-mapper <name>` subcommands for custom mappers. The preferred approach is `tedge-mapper custom --profile <name>`, but first-class subcommands could be added later if demand warrants it.

## Decisions

### D1: Custom mapper directories follow the `custom.{name}` profile convention

Custom mapper instances are named as profiles of the `custom` mapper type, using the existing profile directory convention: `custom.thingsboard`, `custom.mycloud`, `custom.acme-iot`. An unprofiled default is also supported (`custom`), invoked as `tedge-mapper custom`.

This gives us disambiguation for free — built-in mapper directories are plain (`c8y`, `az`, `aws`) or profiled (`c8y.staging`), and custom mapper directories always live under the `custom.*` namespace. There is no need for a special prefix character.

- **Filesystem disambiguation**: `ls /etc/tedge/mappers/` shows `custom.thingsboard/` alongside `c8y/` — the type is visible
- **CLI collision prevention**: `tedge-mapper custom` can never collide with a built-in mapper name
- **Future `tedge config` compatibility**: the `custom.{name}` structure maps naturally to a `custom` section in the global config schema with `#[tedge_config(multi)]`
- **No file reading needed**: detection is purely name-based

**Alternatives considered:**
- Name prefix (`_thingsboard`) — does work, but is unconventional and inhibits searchability; using `custom` as the type name is more self-documenting
- First-class subcommand names (`tedge-mapper thingsboard`) — creates CLI collision risks and prevents using the profile system for instances of the same cloud type

### D2: CLI invocation via `tedge-mapper custom --profile <name>`

Custom mappers are invoked as:

```
tedge-mapper custom --profile thingsboard
tedge-mapper custom                         # uses custom/ (unprofiled)
```

This maps directly onto a new `Custom` variant in `MapperName`:

```rust
MapperName::Custom {
    #[clap(long)]
    profile: Option<ProfileName>,
}
```

clap handles all argument parsing, `--help`, and validation automatically. The mapper name `custom` is a stable, searchable term — users encountering it in logs or docs can look it up immediately. First-class subcommand names (e.g. `tedge-mapper thingsboard`) are aspirational but not pursued in the initial implementation due to CLI collision risks and the complexity of manual argument parsing.

Validation: if `--profile` is provided and no matching `custom.{name}/` directory exists, emit a clear error listing available custom mapper profiles.

### D3: Config is separate from `define_tedge_config!`, with `${mapper.*}` template namespace

A custom mapper's `tedge.toml` (e.g. `/etc/tedge/mappers/custom.thingsboard/tedge.toml`) is **optional** — it is only needed when the mapper establishes a cloud connection via the built-in MQTT bridge. A mapper that only runs flows (local processing, no cloud) needs no `tedge.toml` at all.

When present, the `tedge.toml` is parsed directly by the custom mapper and is invisible to the global `tedge_config`. A minimal example:

```toml
url = "mqtt.thingsboard.io:8883"

[device]
cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"
key_path = "/etc/tedge/device-certs/tedge-private-key.pem"

[bridge]
topic_prefix = "tb"
```

The `url` field is a top-level key using the `HostPort` type (the same `host:port` format used by built-in mapper URLs; port is optional and defaults to 8883). The exact schema for this file is defined by the custom mapper code (D4), not by the config macro. If `bridge/` rules exist but `tedge.toml` is absent, the mapper MUST emit an error — bridge rules require connection details to be useful.

**Bridge template access via `${mapper.*}`**

The bridge template system currently supports `${config.*}` (global config), `${item}` (loop variable), and `${connection.*}` (connection context). For custom mappers, a new `${mapper.*}` namespace resolves against the mapper's own `tedge.toml`:

```toml
# /etc/tedge/mappers/custom.thingsboard/bridge/rules.toml
local_prefix = "${mapper.bridge.topic_prefix}/"
remote_prefix = ""

[[rule]]
topic = "v1/devices/me/telemetry"
direction = "outbound"
```

Implementation: extend `TemplateContext` with a `mapper_config: &toml::Table` field, add `mapper` as a keyword in the template parser alongside `config` and `connection`, and resolve `${mapper.KEY}` by walking the TOML table. The change is localized to the template parsing/expansion code. The `${mapper.*}` namespace is only populated when a `tedge.toml` is present; since `bridge/` without `tedge.toml` is already an error, this is never ambiguous.

Templates can still reference global config via `${config.mqtt.port}` for values shared across all mappers.

This simplification can also be performed on the built-in mapper bridge definitions (though `${config.c8y.bridge.topic_prefix}` would still work).

**Why not `define_tedge_config!`?**
- Custom mapper config is only used by the custom mapper — it doesn't need to be in the global schema compiled into every thin-edge component
- The macro generates compile-time types; custom mappers are a runtime concept
- `${mapper.url}` is shorter and clearer than `${config.custom.thingsboard.url}`

**Trade-off: no `tedge config` integration initially.** Users edit the mapper's `tedge.toml` directly. This is acceptable because the users writing custom mappers are technical enough to edit TOML files. The `custom.{name}` directory structure maps directly to a `custom` section in the global config schema with `#[tedge_config(multi)]`, making future integration straightforward when demand warrants it.

### D4: Custom mapper runtime wires bridge + flows with no built-in converter

A custom mapper is structurally a combination of the flows mapper (`GenMapper`) and the bridge setup from the Cumulocity mapper. The component:

```
CustomMapper {
    profile: Option<ProfileName>,   // e.g. Some("thingsboard") or None
}
```

Its `start()` method, given mapper directory `custom.{name}/` (or `custom/` if unprofiled):

1. Check for `tedge.toml` and `bridge/` in the mapper directory
   - If `bridge/` exists but `tedge.toml` does not → error: bridge rules require connection settings
   - If neither `tedge.toml` nor `flows/` is present → error: no active components to start (an empty mapper directory is not useful and likely a misconfiguration)
2. If `tedge.toml` is present:
   - Parse it to a simple configuration struct; `url` is deserialized as `HostPort` (consistent with built-in mapper URL fields)
   - Extract connection details (host:port from `url`, TLS config, device identity) from the struct
   - Start basic actors (MQTT connection to local broker)
   - Start the built-in MQTT bridge, loading bridge rules from `bridge/` with the mapper's config table available as `${mapper.*}` in templates
3. If `flows/` is present: start the flows engine, loading flows from `flows/`
4. Run to completion

There is no cloud-specific converter. All message transformation is handled by flows. This is the key difference from built-in mappers: the Cumulocity mapper has hardcoded converter logic compiled in (handling SmartREST, operations, entity registration, etc.), while custom mappers rely entirely on user-provided flow scripts.

### D5: Service identity follows existing conventions

Service identity uses the same `{mapper}@{profile}` pattern as built-in profiled mappers. For `tedge-mapper custom --profile thingsboard`:

- Service name: `tedge-mapper-custom@thingsboard`
- Health topic: `te/device/main/service/tedge-mapper-custom@thingsboard/status/health`
- Lock file: `/run/tedge-mapper-custom@thingsboard.lock`
- Bridge service name: `tedge-mapper-bridge-custom@thingsboard`

For the unprofiled case (`tedge-mapper custom`):

- Service name: `tedge-mapper-custom`
- Health topic: `te/device/main/service/tedge-mapper-custom/status/health`
- Lock file: `/run/tedge-mapper-custom.lock`
- Bridge service name: `tedge-mapper-bridge-custom`

### D6: Warn about unrecognised mapper directories

On mapper startup (and optionally on `tedge config` operations), scan `/etc/tedge/mappers/` and warn about directories that are not:

- A known built-in mapper name (`c8y`, `az`, `aws`, `collectd`, `flows`)
- A profile directory of a known mapper (e.g. `c8y.staging`)
- A custom mapper directory (`custom` or `custom.{name}`)

This catches typos like `custom.thingboard` (missing 's') and stale directories from removed mappers.

## Risks / Trade-offs

**`tedge-mapper custom` feels second-class** → Custom mappers are invoked as `tedge-mapper custom --profile thingsboard` rather than `tedge-mapper thingsboard`. Mitigate with clear documentation and good error messages. The `custom` subcommand name is at least obviously searchable — a user encountering it in logs or docs knows where to look.

**No `tedge config` CLI integration initially** → Users must edit TOML files directly for custom mapper settings. This is a departure from the `tedge config set` workflow that built-in mappers support. Acceptable for the initial implementation given the target audience (technical users writing custom integrations), but should be added later.

**Template system extension** → Adding `${mapper.*}` to the bridge template system is a small but cross-cutting change. The parser, context struct, and expansion logic all need updating. Risk is low — the template system is well-structured with clear extension points — but it needs thorough testing.

**Optional `tedge.toml` adds branching** → The mapper start logic branches on whether `tedge.toml` exists, and must error cleanly when `bridge/` is present without `tedge.toml`. This is straightforward but must be well-tested and clearly documented so users understand which components start under which conditions.

## Migration Plan

No migration needed. This is purely additive — new CLI handling, new mapper component, template system extension. No existing configuration, behaviour, or API changes. Users who don't create custom mappers are unaffected.

Rollout:
1. Extend bridge template system with `${mapper.*}` namespace
2. Add `Custom` subcommand variant and `CustomMapper` component
3. Add directory warning logic for unrecognised directories
4. Documentation and examples (ThingsBoard walkthrough)

Rollback: remove the `Custom` CLI variant and template extension. No data migration concerns since `custom.{name}/` directories are user-created.
