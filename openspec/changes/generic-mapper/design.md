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

- Users can create a custom mapper by creating a directory under `/etc/tedge/mappers/` with a `tedge.toml`, bridge rules, and flow scripts
- A custom mapper runs the built-in MQTT bridge and flows engine as a single service, identical to how the Cumulocity mapper runs them
- Multiple custom mappers can coexist independently
- Built-in mappers are completely unaffected

**Non-Goals:**

- `tedge config` CLI integration for custom mapper settings (users edit their mapper's `tedge.toml` directly — the less-technical users `tedge config` targets are unlikely to be writing custom mappers)
- Profile support for custom mappers (design accommodates it, implementation deferred)
- Custom mappers appearing in `tedge-mapper --help` and shell completions (incremental follow-up)
- A packaging/distribution format for custom mappers
- Custom code-based converters (all conversion is via flows)

**Aspirational (may or may not be worth pursuing):**

- First-class `tedge-mapper <name>` subcommands for custom mappers, so they feel identical to built-in mappers. This would be ideal UX, but may pose insurmountable implementation hurdles (see D2). If it proves too costly, `tedge-mapper custom --profile <name>` is an acceptable fallback

## Decisions

### D1: Custom mapper names use `_` prefix

Custom mapper directories are named with a `_` prefix: `_thingsboard`, `_mycloud`, `_acme-iot`.

The prefix is the signal — built-in mapper names are always plain (`c8y`, `az`, `aws`), custom names are always prefixed. This gives us:

- **Filesystem disambiguation**: `ls /etc/tedge/mappers/` immediately shows which are custom
- **CLI collision prevention**: `tedge-mapper _thingsboard` can never collide with a future built-in subcommand since clap variants don't start with `_`
- **No file reading needed**: detection is purely name-based, no marker files or TOML parsing

`_` is chosen over `+` because it is valid in all filesystems and needs no shell escaping.

**Alternatives considered:**
- No prefix, with custom-wins precedence rule — doesn't work cleanly because a built-in mapper with the same name would create its own configuration alongside the custom mapper's, resulting in a mixture of built-in and custom behaviour
- Marker file (`.custom`) — fragile, easily deleted, requires directory scanning
- Config field (`type = "custom"`) — requires parsing every mapper's config to determine its type

### D2: CLI invocation

There are two viable approaches, with different trade-offs.

**Preferred: First-class subcommand via clap `external_subcommand`**

Add a catch-all variant to `MapperName` using clap's `#[command(external_subcommand)]`:

```rust
#[derive(Debug, clap::Subcommand, Clone)]
pub enum MapperName {
    // ... existing variants unchanged ...
    #[command(external_subcommand)]
    Custom(Vec<OsString>),
}
```

When `tedge-mapper _thingsboard` is invoked and `_thingsboard` doesn't match any built-in variant, clap captures it via the `Custom` variant. The first element is the mapper name; any flags (e.g. `--profile`) would need to be extracted manually from the remaining arguments.

Validation on the `Custom` variant:
1. Mapper name must start with `_`
2. A matching directory must exist under `/etc/tedge/mappers/`
3. If neither condition is met, emit a clear error listing available mappers (both built-in and discovered custom ones)

This gives the best UX — custom mappers look and feel like built-in ones. However, it has implementation risks: manual argument parsing for flags, no `--help` integration, and potential edge cases with clap's external subcommand handling. These may prove too costly to solve well.

**Fallback: Dedicated `custom` subcommand**

If first-class subcommands prove too difficult:

```rust
MapperName::Custom {
    #[clap(long)]
    name: String,  // or positional
    #[clap(long)]
    profile: Option<ProfileName>,
}
```

Invoked as `tedge-mapper custom --profile _thingsboard`. This is straightforward to implement — clap handles all argument parsing, help text, and validation. The trade-off is that custom mappers feel second-class. However, it is significantly more searchable and discoverable — a user seeing `tedge-mapper custom` in docs or help output immediately knows where to look.

The design should proceed with the fallback approach for the initial implementation, with the first-class approach as an upgrade if it proves feasible.

### D3: Config is separate from `define_tedge_config!`, with `${mapper.*}` template namespace

Custom mapper config lives entirely in the mapper's own directory (`/etc/tedge/mappers/_thingsboard/tedge.toml`), separate from the global `define_tedge_config!` schema interacted with through the `tedge config` CLI. The global config schema is large because it's referenced throughout thin-edge; custom mapper config is isolated (only the custom mapper itself uses it), so pulling it out is both sensible and simpler.

The mapper's `tedge.toml` is parsed directly by the custom mapper, and is invisible to `tedge_config`. A minimal example:

```toml
[connection]
url = "mqtt.thingsboard.io"
port = 8883

[device]
cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"
key_path = "/etc/tedge/device-certs/tedge-private-key.pem"

[bridge]
topic_prefix = "tb"
```

The exact schema for this file is defined by the custom mapper code (D4), not by the config macro.

**Bridge template access via `${mapper.*}`**

The bridge template system currently supports `${config.*}` (global config), `${item}` (loop variable), and `${connection.*}` (connection context). For custom mappers, a new `${mapper.*}` namespace resolves against the mapper's own `tedge.toml`:

```toml
# /etc/tedge/mappers/_thingsboard/bridge/rules.toml
local_prefix = "${mapper.bridge.topic_prefix}/"
remote_prefix = ""

[[rule]]
topic = "v1/devices/me/telemetry"
direction = "outbound"
```

Implementation: extend `TemplateContext` with a `mapper_config: &toml::Table` field, add `mapper` as a keyword in the template parser alongside `config` and `connection`, and resolve `${mapper.KEY}` by walking the TOML table. The change is localized to the template parsing/expansion code.

Templates can still reference global config via `${config.mqtt.port}` for values shared across all mappers.

This simplification can also be performed on the built in mapper bridge definitions (though `${config.c8y.bridge.topic_prefix}` would still work).

**Why not `define_tedge_config!`?**
- Custom mapper config is only used by the custom mapper — it doesn't need to be in the global schema that's compiled into every thin-edge component
- The macro generates compile-time types; custom mappers are a runtime concept
- Avoids the question of how multi-profile keys interact with custom mapper names
- `${mapper.url}` is shorter and clearer than `${config.custom._thingsboard.url}`

**Trade-off: no `tedge config` integration.** Users cannot use `tedge config set/get` for custom mapper settings — they edit the mapper's `tedge.toml` directly. This is acceptable because (a) the users writing custom mappers are technical enough to edit TOML files, (b) it avoids complex machinery to bridge dynamic config into the typed system, and (c) `tedge config` support can _potentially_ be added later if demand warrants it.

### D4: Custom mapper runtime wires bridge + flows with no built-in converter

A custom mapper is structurally a combination of the flows mapper (`GenMapper`) and the bridge setup from the Cumulocity mapper. The component:

```
CustomMapper {
    name: String,           // e.g. "_thingsboard"
}
```

Its `start()` method:

1. Parse the mapper's `tedge.toml` from `/etc/tedge/mappers/{name}/tedge.toml` to a simple configuration struct replicating the shared configuration from the existing mappers
2. Extract connection details (URL, TLS config, device identity) from the table
3. Start basic actors (MQTT connection to local broker)
4. Start the built-in MQTT bridge, loading bridge rules from `/etc/tedge/mappers/{name}/bridge/` with the mapper's config table available as `${mapper.*}` in templates
5. Start the flows engine, loading flows from `/etc/tedge/mappers/{name}/flows/`
6. Run to completion

There is no cloud-specific converter. All message transformation is handled by flows. This is the key difference from built-in mappers: the Cumulocity mapper has hardcoded converter logic compiled in (handling SmartREST, operations, entity registration, etc.), while custom mappers rely entirely on user-provided flow scripts.

### D5: Service identity follows existing conventions

- Service name: `tedge-mapper-_thingsboard` (consistent with `tedge-mapper-c8y`)
- Health topic: `te/device/main/service/tedge-mapper-_thingsboard/status/health`
- Lock file: `/run/tedge-mapper-_thingsboard.lock`
- Bridge service name: `tedge-mapper-bridge-_thingsboard`

### D6: Warn about unrecognised mapper directories

On mapper startup (and optionally on `tedge config` operations), scan `/etc/tedge/mappers/` and warn about directories that are not:

- A known built-in mapper name (`c8y`, `az`, `aws`, `collectd`, `flows`)
- A profile directory of a known mapper (e.g. `c8y.staging`)
- A custom mapper directory (starts with `_`)

This catches typos like `_thingboard` (missing 's') and stale directories from removed mappers.

## Risks / Trade-offs

**`_` prefix is unconventional** → Users may find it surprising or ugly. Mitigate with clear documentation, good error messages ("mapper name must start with '_'"), and examples.

**`_` prefix inhibits searchability** → A user encountering `tedge-mapper _thingsboard` in logs or scripts may not know what to search for in the docs. This is particularly true compared to the more verbose `tedge-mapper custom --name _thingsboard`, where `custom` is an obvious search term. Mitigate with clear help text in the `tedge-mapper` command itself (e.g. "Custom mappers use names starting with '_'. See <docs link>.") and ensuring "custom mapper" appears prominently in documentation for the `_` prefix convention.

**No `tedge config` CLI integration** → Users must edit TOML files directly for custom mapper settings. This is a departure from the `tedge config set` workflow that built-in mappers support. Acceptable for the initial implementation given the target audience (technical users writing custom integrations), but could be added later if there is demand.

**Template system extension** → Adding `${mapper.*}` to the bridge template system is a small but cross-cutting change. The parser, context struct, and expansion logic all need updating. Risk is low — the template system is well-structured with clear extension points — but it needs thorough testing.

**No profile support initially** → Users connecting to two instances of the same custom cloud must create two separate custom mapper directories (`_thingsboard-prod`, `_thingsboard-staging`) rather than using profiles. The directory structure naturally accommodates profiles later (`_thingsboard.staging/`).

## Migration Plan

No migration needed. This is purely additive — new CLI handling, new mapper component, template system extension. No existing configuration, behaviour, or API changes. Users who don't create custom mappers are unaffected.

Rollout:
1. Extend bridge template system with `${mapper.*}` namespace
2. Add `Custom` subcommand variant and `CustomMapper` component
3. Add directory warning logic
4. Documentation and examples (ThingsBoard walkthrough)

Rollback: remove the `Custom` CLI variant and template extension. No data migration concerns since custom mapper directories are user-created.
