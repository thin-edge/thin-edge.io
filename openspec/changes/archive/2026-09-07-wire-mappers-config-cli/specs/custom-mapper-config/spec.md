## MODIFIED Requirements

### Requirement: Custom mapper config is separate from global tedge_config
User-defined mapper configuration SHALL NOT be part of the `define_tedge_config!` macro
or the global `tedge_config` schema.

When the `mapper-config` cargo feature is enabled,
users SHALL be able to read and write user-defined mapper configuration
via `tedge config get/set/unset` using `mappers.<name>.<key>` keys.
These keys are routed to the `tedge_config_engine` `FederatedConfig`,
not to the `define_tedge_config!` schema.
`tedge config list` SHALL include user-defined mapper keys
when the feature is enabled.

When the `mapper-config` feature is disabled,
`tedge config` commands SHALL NOT see user-defined mapper settings
(preserving the current behaviour).

Users MAY also configure user-defined mappers
by editing `mapper.toml` directly, as before.

#### Scenario: tedge config sees mapper settings when feature enabled
- **WHEN** the `mapper-config` feature is enabled
  and `/etc/tedge/mappers/thingsboard/mapper.toml` exists
  and a user runs `tedge config list`
- **THEN** the output SHALL include keys prefixed with `mappers.thingsboard.`

#### Scenario: tedge config does not see mapper settings when feature disabled
- **WHEN** the `mapper-config` feature is not enabled
  and a user runs `tedge config list`
- **THEN** the output SHALL NOT include any `mappers.*` keys

#### Scenario: User edits mapper config directly
- **WHEN** a user edits `/etc/tedge/mappers/thingsboard/mapper.toml` to change `url`
- **THEN** the change takes effect the next time the mapper is started
  (no `tedge config set` needed)

#### Scenario: tedge config set writes to mapper.toml
- **WHEN** the `mapper-config` feature is enabled
  and a user runs `tedge config set mappers.thingsboard.url mqtt.thingsboard.io:8883`
- **THEN** the value SHALL be written to `/etc/tedge/mappers/thingsboard/mapper.toml`
  (not to `tedge.toml`)
