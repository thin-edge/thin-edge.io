## Why

Users currently configure custom mappers by editing `mapper.toml` files directly.
There is no validation at edit time, no way to discover available keys,
and no consistent CLI experience across built-in and user-defined mappers.
The facet-powered config engine (PR 1) was built to solve this,
and wiring it into the CLI is the next step.

## Proposed solution

`tedge config` gains support for `mappers.*` keys,
letting users read and write custom mapper configuration through the same CLI
they already use for core thin-edge settings:

```bash
tedge config set mappers.thingsboard.url mqtt.thingsboard.io:8883
tedge config get mappers.thingsboard.device.cert_path
tedge config list   # now includes mappers.thingsboard.* keys
tedge config unset mappers.thingsboard.bridge.max_payload_size

TEDGE_CONFIG_MAPPERS_THINGSBOARD_URL=override tedge config get mappers.thingsboard.url
```

Mapper-level `device.*` keys that are not explicitly set
fall back to the root `tedge.toml` values (e.g. `device.cert_path`),
matching the existing runtime behaviour.

Unknown mapper names produce a helpful error listing the mappers found on disk.

This is behind a cargo feature flag (`mapper-config`) and does not change
any existing `tedge config` behaviour for non-`mappers.*` keys.

## What Changes

- `tedge config get/set/unset/add/remove` accept `mappers.<name>.<key>` keys,
  routed to the new `tedge_config_engine` via `FederatedConfig`.
- `tedge config list` includes keys from all discovered mapper configs.
- A new `tedge_mapper_config` crate defines the mapper config schema
  using the `define_config!` macro,
  with `from_root` fallback for `device.cert_path`, `device.key_path`,
  and `device.root_cert_path`.
- A temporary root config stub exposes the keys referenced by `from_root`;
  replaced by the full schema in a later PR.
- Environment variable overrides: `TEDGE_CONFIG_MAPPERS_<NAME>_<KEY>`.
- All changes are behind the `mapper-config` cargo feature (default off).
- Existing `tedge config` behaviour for non-`mappers.*` keys is unchanged.

## Capabilities

### New Capabilities

- `config-engine-cli-routing`: Routing `mappers.*` keys from the `tedge config`
  CLI commands to the `tedge_config_engine` `FederatedConfig` backend,
  including mapper discovery, environment variable overrides, and error diagnostics.

### Modified Capabilities

- `custom-mapper-config`: The "Custom mapper config is separate from global tedge_config"
  requirement changes.
  Custom mapper config becomes accessible through `tedge config get/set`
  under the `mappers.*` key prefix,
  in addition to direct `mapper.toml` editing.
  The `tedge config list` exclusion for mapper settings is removed
  when the `mapper-config` feature is enabled.

## Impact

- **New crate**: `crates/common/tedge_mapper_config/` (mapper schema + root stub + assembly).
- **Modified crate**: `crates/core/tedge/` gains `tedge_config_engine` and `tedge_mapper_config`
  dependencies behind the `mapper-config` feature.
- **Modified files**: each CLI command handler in `crates/core/tedge/src/cli/config/commands/`
  gains a `mappers.*` routing branch.
- **No runtime impact**: mappers themselves continue reading config through `CustomMapperConfig`.
  The CLI integration is read/write to the TOML files only.
