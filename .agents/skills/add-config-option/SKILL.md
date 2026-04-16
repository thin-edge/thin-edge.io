---
name: add-config-option
description: Add a new configuration option to tedge_config. Use when adding a new setting to the thin-edge.io configuration in tedge.toml.
---

# Add Configuration Option

## Steps

1. **Add field** to `define_tedge_config!` macro in `crates/common/tedge_config/src/tedge_toml/tedge_config.rs`
1. **Choose appropriate type**:
   - `String` — free-form text
   - `AbsolutePath` — filesystem path
   - `ConnectUrl` — URL with scheme
   - `SecondsOrHumanTime` — duration (e.g., `30s`, `5m`)
   - Custom types in `crates/common/tedge_config/src/tedge_toml/models/`
1. **Add `#[tedge_config()]` attributes**:
   - `default(value = "...")` — default value
   - `example = "..."` — example for documentation
   - `reader(...)` — custom reader function
   - Other modifiers as needed
1. **Add doc comment** above the field — this becomes user-facing help text in `tedge config`
1. **For custom types**, add to `crates/common/tedge_config/src/tedge_toml/models/`
1. **Verify**: Run `cargo test -p tedge_config`

## Reference Files

Read these for patterns:
- `crates/common/tedge_config/src/tedge_toml/tedge_config.rs` — the `define_tedge_config!` invocation with all existing options
- `crates/common/tedge_config/src/tedge_toml/models/` — custom configuration types
