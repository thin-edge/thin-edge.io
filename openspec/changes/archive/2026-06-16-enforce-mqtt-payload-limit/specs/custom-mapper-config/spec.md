## MODIFIED Requirements

### Requirement: Custom mapper configuration file
When present, a mapper's `mapper.toml` SHALL contain connection and device identity settings needed to establish the MQTT bridge to the cloud, plus an optional `cloud_type` field identifying the built-in cloud integration this mapper instance targets. The `mapper.toml` is OPTIONAL — it is only required when the mapper establishes a cloud connection via the built-in MQTT bridge.

For user-defined mappers, `mapper.toml` is parsed directly and is not part of the global `tedge_config` schema. Certain fields (device cert and key paths) are inherited from the root `tedge.toml` when absent from `mapper.toml`.

For built-in mappers (`c8y`, `az`, `aws`), `mapper.toml` is backed by `TEdgeConfig` and values are written via `tedge config set`. Built-in mappers are not required to have a `mapper.toml` on disk — the file is only created when the mapper configuration is upgraded via `tedge config upgrade`. Until upgraded, built-in mappers read their configuration directly from the root `tedge.toml`.

The `[bridge]` section MAY contain a `max_payload_size` field setting the maximum MQTT packet size the bridge will forward to the cloud for this mapper. When absent, it SHALL default to the MQTT maximum packet size, leaving the limit effectively disabled for brokers whose limit is unknown.

#### Scenario: Configuration with connection details
- **WHEN** a user-defined mapper's `mapper.toml` contains a top-level `url` field and a `[device]` section with `cert_path` and `key_path` fields
- **THEN** the mapper SHALL use these values to configure the MQTT bridge connection

#### Scenario: Device cert fields inherited from root tedge.toml
- **WHEN** a user-defined mapper's `mapper.toml` does not contain `[device] cert_path` or `key_path` and the root `tedge.toml` has these fields configured
- **THEN** the mapper SHALL use the root `tedge.toml` values as fallback

#### Scenario: Configuration with additional custom fields
- **WHEN** a mapper's `mapper.toml` contains additional TOML keys beyond the required connection and device settings (e.g. `[bridge]` with `topic_prefix`)
- **THEN** the mapper SHALL make all fields available via the `${mapper.*}` template namespace in bridge rule templates

#### Scenario: Payload size limit configured for a user-defined mapper
- **WHEN** a user-defined mapper's `mapper.toml` contains a `[bridge]` section with `max_payload_size`
- **THEN** the bridge for that mapper SHALL enforce that limit on messages forwarded to the cloud

#### Scenario: Payload size limit defaults when unset
- **WHEN** a user-defined mapper's `mapper.toml` does not set `max_payload_size`
- **THEN** the bridge SHALL use the MQTT maximum packet size as the limit

#### Scenario: Invalid TOML in configuration file
- **WHEN** a mapper's `mapper.toml` contains invalid TOML syntax
- **THEN** `tedge-mapper` SHALL report a parse error with the file path and error location

#### Scenario: cloud_type field present in user-defined mapper
- **WHEN** a user-defined mapper's `mapper.toml` contains `cloud_type = "c8y"`
- **THEN** `tedge mapper list` SHALL display `cloud_type=c8y` alongside that mapper's name

#### Scenario: cloud_type field absent
- **WHEN** a mapper's `mapper.toml` does not contain a `cloud_type` field
- **THEN** `tedge mapper list` SHALL display the mapper without a cloud type annotation

#### Scenario: Built-in mapper.toml pre-populated with cloud_type
- **WHEN** the built-in `c8y` mapper directory is inspected after `tedge config upgrade`
- **THEN** its `mapper.toml` SHALL contain `cloud_type = "c8y"`

#### Scenario: Built-in mapper without mapper.toml
- **WHEN** the built-in `c8y` mapper has not been upgraded via `tedge config upgrade`
- **THEN** no `c8y/mapper.toml` file need exist — the mapper reads its configuration from the root `tedge.toml`
