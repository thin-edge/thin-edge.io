## Why

A mapper in thin-edge boils down to two things: an MQTT bridge connecting the local broker to a cloud IoT platform, and logic to convert between thin-edge messages (measurements, events, alarms) and cloud-specific messages (and vice versa for operations/RPC). Today, the built-in mappers for Cumulocity, Azure IoT, and AWS handle both of these, but users wanting to connect to other platforms (e.g. ThingsBoard) would need to write and maintain their own separate mapper application.

All the building blocks to make this configurable now exist or are in progress:

- **Flows** allow users to write custom mapping rules in JavaScript. There is already proof-of-concept work simulating a ThingsBoard connection using flows, and flows can be run today via `tedge-mapper flows` (soon to be `tedge-mapper local`).
- **The built-in MQTT bridge** replaces the previous reliance on mosquitto for bridging. It lives inside tedge-mapper, supports proxied cloud connections, and makes thin-edge broker-agnostic. A recent PR added support for configuring bridge rules via a TOML file with basic templating, which the Cumulocity mapper already uses.

The built-in mappers already use a unified directory structure per mapper (e.g. `/etc/tedge/mappers/c8y/` contains `tedge.toml`, `bridge/`, and `flows/` side by side). What's missing is the ability for users to create their own mapper using this same pattern — defining a new mapper directory with connection settings, bridge rules, and flows, and having thin-edge run it as a first-class service. This change makes that possible so users can create custom cloud mappers without writing any Rust code — only thin-edge configuration files and flow scripts.

## What Changes

- Users can define a custom mapper by creating a directory under `/etc/tedge/mappers/custom.{name}/` (following the same structure as built-in mapper profiles) containing any combination of: connection settings (`tedge.toml`), bridge rules (`bridge/`), and flow scripts (`flows/`)
- Users can configure custom mapper connection details (cloud URL, certificates, device identity) by editing the mapper's `tedge.toml` directly (not via the `tedge config` CLI — see open questions)
- Users can start a custom mapper with `tedge-mapper custom --profile <name>`, which launches the built-in MQTT bridge (if `tedge.toml` is present) and flows engine (if `flows/` is present) as a single service
- Multiple custom mappers can coexist as separate profiles, each with its own configuration, bridge rules, and flows
- No changes to existing built-in mappers (c8y, az, aws)

## Open Questions

### How should the config namespace work?

Custom mapper config is isolated — only the custom mapper itself uses it, unlike the global `tedge_config` schema which is referenced throughout thin-edge. For now, custom mapper settings live in the mapper's own `tedge.toml` (e.g. `/etc/tedge/mappers/custom.thingsboard/tedge.toml`), separate from the `define_tedge_config!` macro. Users edit this file directly rather than using `tedge config set/get`.

This avoids the complexity of fitting dynamic/unknown mapper names into the compile-time config macro. The bridge template system needs a new `${mapper.*}` variable namespace to let bridge rule templates reference the mapper's own config. `tedge config` integration is potentially important and should be added later — the `custom.{name}` directory structure maps naturally to a `custom` section in the global config schema with `#[tedge_config(multi)]`, making future integration straightforward.

### Discoverability

Can custom mappers appear in `tedge-mapper --help` and shell completions? clap doesn't natively support dynamic subcommands in help text, but custom mapper profiles could be listed by scanning the mappers directory. Tab-completion could also discover custom mapper profiles by scanning the filesystem. Both are nice-to-haves that can be added incrementally.

## Capabilities

### New Capabilities
- `custom-mapper-config`: How users configure custom mappers — the settings available (connection URL, certificates, device identity), the directory layout (mapper `tedge.toml`, bridge rules, and flows), and how bridge templates reference mapper-local config via `${mapper.*}`
- `custom-mapper-runtime`: How users run custom mappers — CLI invocation, service lifecycle, what components are started (bridge, flows, health monitoring), how the mapper appears in the system, and name collision handling

### Modified Capabilities
None. Built-in mappers (c8y, az, aws) are unaffected.

## Impact

- **User-facing**: New `tedge-mapper custom --profile <name>` command; new mapper service instances; custom mapper config edited directly in the mapper's `tedge.toml` (if present)
- **Configuration**: Custom mapper profile directories under `/etc/tedge/mappers/custom.{name}/` alongside existing mapper directories; no changes to the global `tedge_config` schema (deferred)
- **tedge_mqtt_bridge crate**: New `${mapper.*}` template variable namespace for bridge rule templates
- **tedge_mapper crate**: New CLI subcommand handling, new mapper component wiring up the bridge and flows
- **No breaking changes** to existing configuration or behavior
