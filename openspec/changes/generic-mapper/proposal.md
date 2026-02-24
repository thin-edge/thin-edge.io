## Why

A mapper in thin-edge boils down to two things: an MQTT bridge connecting the local broker to a cloud IoT platform, and logic to convert between thin-edge messages (measurements, events, alarms) and cloud-specific messages (and vice versa for operations/RPC). Today, the built-in mappers for Cumulocity, Azure IoT, and AWS handle both of these, but users wanting to connect to other platforms (e.g. ThingsBoard) would need to write and maintain their own separate mapper application.

All the building blocks to make this configurable now exist or are in progress:

- **Flows** allow users to write custom mapping rules in JavaScript. There is already proof-of-concept work simulating a ThingsBoard connection using flows, and flows can be run today via `tedge-mapper flows` (soon to be `tedge-mapper local`).
- **The built-in MQTT bridge** replaces the previous reliance on mosquitto for bridging. It lives inside tedge-mapper, supports proxied cloud connections, and makes thin-edge broker-agnostic. A recent PR added support for configuring bridge rules via a TOML file with basic templating, which the Cumulocity mapper already uses.

The built-in mappers already use a unified directory structure per mapper (e.g. `/etc/tedge/mappers/c8y/` contains `tedge.toml`, `bridge/`, and `flows/` side by side). What's missing is the ability for users to create their own mapper using this same pattern — defining a new mapper directory with connection settings, bridge rules, and flows, and having thin-edge run it as a first-class service. This change makes that possible so users can create custom cloud mappers without writing any Rust code — only thin-edge configuration files and flow scripts.

## What Changes

- Users can define a custom mapper by creating a configuration directory (following the same structure as built-in mappers) with connection settings, bridge rules, and flows
- Users can configure custom mapper connection details (cloud URL, certificates, device identity) by editing the mapper's `tedge.toml` directly (not via the `tedge config` CLI — see open questions)
- Users can start a custom mapper with `tedge-mapper <name>`, which launches the built-in MQTT bridge and flows engine as a single service
- Multiple custom mappers can coexist, each with its own configuration, bridge rules, and flows
- No changes to existing built-in mappers (c8y, az, aws)

## Open Questions

These questions affect the user-facing design and need to be resolved before or during the design phase.

### How should custom mappers be invoked?

Two main approaches, with different trade-offs:

**Option A: Dedicated `custom` subcommand with profiles.** `tedge-mapper custom --profile thingsboard`. Simple to implement — the profile system already exists. But it makes custom mappers feel second-class compared to `tedge-mapper c8y`, and it prevents users from using profiles for their intended purpose (e.g. connecting to multiple ThingsBoard instances).

**Option B: Custom mapper name as a first-class subcommand.** `tedge-mapper thingsboard`. The user experience is identical to built-in mappers, and profiles work naturally (`tedge-mapper thingsboard --profile staging`). This is more elegant, but raises further questions (see below).

We favour Option B for the user experience, but it needs solutions for disambiguation, name collisions, and discoverability.

### How do we disambiguate custom and built-in mappers?

With Option B, both custom and built-in mappers live under `/etc/tedge/mappers/` and are invoked the same way. We need a reliable way to tell them apart, both on the filesystem and in CLI parsing:

- **On the filesystem**: If thin-edge later adds a built-in `thingsboard` mapper, there's nothing intrinsic about `/etc/tedge/mappers/thingsboard/` that says whether it's custom or built-in. We need a signal.
- **In CLI parsing**: clap parses built-in subcommands first. If we later add a built-in `Thingsboard` variant, clap will match it directly, and we'd need fallback logic to check whether the user actually has a custom mapper with that name.

Possible disambiguation approaches:

1. **Name prefix convention**: Custom mapper directories use a prefix character (e.g. `_thingsboard` or `+thingsboard`). The prefix is the signal — built-in names never have it. This is filesystem-visible, unambiguous, and avoids the clap fallback problem entirely since `tedge-mapper +thingsboard` can never collide with a built-in subcommand. The config file/directory naming could follow the same convention.

2. **Different config file name**: Custom mappers use e.g. `mapper.toml` instead of `tedge.toml`. Detection requires reading the directory contents rather than just the name, making it less robust.

3. **Marker file**: A `.custom` file in the directory. Simple but fragile — easy to accidentally delete.

4. **Config field**: A `type = "custom"` field in the mapper's `tedge.toml`. Requires parsing the file to determine the mapper type.

A name prefix (option 1) seems strongest — it's visible in `ls`, requires no file reading, and naturally prevents collisions with both current and future built-in names. The choice of prefix character matters: `_` is filesystem-friendly everywhere; `+` is clearly intentional but may need shell escaping.

### How do we handle name collisions with future built-in mappers?

With a prefix convention, collisions are structurally prevented — built-in names are plain (e.g. `c8y`), custom names are prefixed (e.g. `_thingsboard`), and they can never overlap. If thin-edge later ships a built-in `thingsboard` mapper, it doesn't affect the user's `_thingsboard` custom mapper at all.

Without a prefix, we would need a precedence rule. In that case, **custom should win** — the user has explicitly set up their mapper, and silently overriding it with a new built-in would be surprising and could break their deployment. A warning should be emitted when a custom mapper shadows a built-in name, so the user is aware and can rename if they want the built-in behaviour. Naming guidance in docs (e.g. recommend org-prefixed names like `mycompany-thingsboard`) could reduce the likelihood of collisions in practice. A reserved names list is impractical since the possible names are too arbitrary.

### How do we handle unrecognised directories under `/etc/tedge/mappers/`?

With this design, there will be a mix of built-in mapper directories, custom mapper directories, and potentially profile directories (e.g. `c8y.staging/`) under `/etc/tedge/mappers/`. Typos or stale directories could easily cause confusion — a user might create `/etc/tedge/mappers/thingboard/` (missing an 's') and wonder why their mapper isn't working. thin-edge should warn about any directories under `/etc/tedge/mappers/` that it doesn't recognise as either a built-in mapper, a custom mapper, or a valid profile directory, to help users catch mistakes early.

### Should custom mappers support profiles from day one?

With Option B, profiles work naturally (`tedge-mapper _thingsboard --profile staging` → `/etc/tedge/mappers/_thingsboard.staging/`). The design should account for how profiles would work with custom mappers, but the initial implementation can defer profile support and add it later.

### How should the config namespace work?

Custom mapper config is isolated — only the custom mapper itself uses it, unlike the global `tedge_config` schema which is referenced throughout thin-edge. This suggests custom mapper settings should live in the mapper's own `tedge.toml` (e.g. `/etc/tedge/mappers/_thingsboard/tedge.toml`), separate from the `define_tedge_config!` macro. Users edit this file directly rather than using `tedge config set/get`.

This avoids the complexity of fitting dynamic/unknown mapper names into the compile-time config macro, and is acceptable because the users writing custom mappers are technical enough to edit TOML files. The bridge template system would need a new `${mapper.*}` variable namespace to let bridge rule templates reference the mapper's own config (analogous to how `${config.c8y.url}` references global config today). `tedge config` support could potentially be added later if demand warrants it (though this might prove complex to implement in practice).

### Discoverability

Can custom mappers appear in `tedge-mapper --help` and shell completions? clap doesn't natively support dynamic subcommands in help text, but custom mappers could be listed in a separate section (e.g. "Custom mappers:" appended to help output by scanning the mappers directory). Tab-completion could also discover custom mappers by scanning the filesystem. Both are nice-to-haves that can be added incrementally.

## Capabilities

### New Capabilities
- `custom-mapper-config`: How users configure custom mappers — the settings available (connection URL, certificates, device identity), the directory layout (mapper `tedge.toml`, bridge rules, and flows), and how bridge templates reference mapper-local config via `${mapper.*}`
- `custom-mapper-runtime`: How users run custom mappers — CLI invocation, service lifecycle, what components are started (bridge, flows, health monitoring), how the mapper appears in the system, and name collision handling

### Modified Capabilities
None. Built-in mappers (c8y, az, aws) are unaffected.

## Impact

- **User-facing**: New `tedge-mapper <name>` command (or `tedge-mapper custom --profile <name>`); new mapper service instances; custom mapper config edited directly in the mapper's `tedge.toml`
- **Configuration**: Custom mapper directories under `/etc/tedge/mappers/` alongside existing mapper directories; no changes to the global `tedge_config` schema
- **tedge_mqtt_bridge crate**: New `${mapper.*}` template variable namespace for bridge rule templates
- **tedge_mapper crate**: New CLI subcommand handling, new mapper component wiring up the bridge and flows
- **No breaking changes** to existing configuration or behavior
