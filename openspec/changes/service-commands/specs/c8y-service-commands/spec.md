## ADDED Requirements

### Requirement: The service commands feature is enabled with `c8y.enable.service_commands` config

thin-edge SHALL provide the `c8y.enable.service_commands` configuration,
which enables the service command feature of the mapper.
It SHALL be enabled by default.

While it is disabled, the mapper SHALL NOT register `c8y_ServiceCommand` as a supported operation of a service,
SHALL NOT publish `c8y_SupportedServiceCommands`,
and SHALL ignore a received `c8y_ServiceCommand` operation.
Ignoring the operation is logged as a warning.

#### Scenario: The feature is enabled by default
- **WHEN** `c8y.enable.service_commands` is not set
- **THEN** the mapper SHALL handle the capabilities and the operations of services as specified below

#### Scenario: No action is declared while the feature is disabled
- **WHEN** `c8y.enable.service_commands` is `false`
  and a service publishes its `cmd/control` capability
- **THEN** the mapper SHALL NOT register `c8y_ServiceCommand` for that service
  and SHALL NOT publish `c8y_SupportedServiceCommands`

#### Scenario: An operation is ignored while the feature is disabled
- **WHEN** `c8y.enable.service_commands` is `false`
  and the mapper receives a `c8y_ServiceCommand` operation
- **THEN** the mapper SHALL ignore it and not publish a new command

### Requirement: The mapper registers c8y_ServiceCommand as a supported operation of a service

When the c8y mapper receives the `cmd/control` capability of a service,
it SHALL register `c8y_ServiceCommand` as a supported operation of that service,
using the same supported-operation mechanism it uses today (SmartREST `114`).

This registration SHALL NOT require an operation file
to be provided by the service or by an administrator:
the mapper SHALL create the supported operation entry itself
under `/etc/tedge/operations/c8y/<service-external-id>/`
and SHALL reload that service's operations,
since the operation directory of a service is not watched for changes.

#### Scenario: A service declaring an action becomes actionable in the cloud
- **WHEN** a service publishes its `cmd/control` capability
- **THEN** the mapper SHALL declare `c8y_ServiceCommand` among that service's supported operations

#### Scenario: No operation file has to be provided
- **WHEN** a service declares its capabilities over MQTT only
- **THEN** the mapper SHALL register the supported operation on its own,
  with nothing to install beforehand

### Requirement: The mapper publishes the declared actions as c8y_SupportedServiceCommands

The c8y mapper SHALL publish the `actions` of the `cmd/control` capability of a service
as the `c8y_SupportedServiceCommands` array on the service's managed object,
through an inventory update over the JSON over MQTT API.

Action names SHALL be published as declared, keeping their case and their order.

An action whose name does not match the action name rule `[A-Za-z][A-Za-z0-9_-]*`
SHALL NOT be published, and SHALL only be logged.
Cumulocity would otherwise offer a command which the mapper refuses
when an operator triggers it.

The array SHALL be re-published whenever the `control` capability changes.
When the `control` capability topic is cleared, or its `actions` array is empty,
the mapper SHALL publish an empty `c8y_SupportedServiceCommands` array.

#### Scenario: Actions are published as declared
- **WHEN** a service declares `{"actions": ["start", "stop", "requestLog"]}`
- **THEN** the mapper SHALL set `{"c8y_SupportedServiceCommands": ["start", "stop", "requestLog"]}`
  on that service's managed object

#### Scenario: An action name breaking the rule is not published
- **WHEN** a service declares `{"actions": ["restart", "do something"]}`
- **THEN** only `restart` SHALL appear in that service's `c8y_SupportedServiceCommands`,
  and `do something` SHALL be logged as dropped

#### Scenario: All actions are removed
- **WHEN** a service clears its `cmd/control` capability topic
- **THEN** the mapper SHALL publish an empty `c8y_SupportedServiceCommands` array,
  so that Cumulocity stops offering actions for that service

### Requirement: The other commands of a service are mapped as they are for a device

The c8y mapper SHALL map every command capability of a service other than `cmd/control`
as it maps the same capability of a device,
and SHALL map the operations and the command states of those commands the same way.

#### Scenario: A service declares an operation
- **WHEN** a service publishes `{"types": ["journald"]}` on its `cmd/log_upload` topic
- **THEN** the mapper SHALL register `c8y_LogfileRequest` among the supported operations of that service,
  and SHALL NOT add `log_upload` to its `c8y_SupportedServiceCommands`

#### Scenario: An operation on a service becomes a command of that service
- **WHEN** the mapper receives a `c8y_LogfileRequest` operation for that service
- **THEN** it SHALL publish a `log_upload` command on the `cmd/log_upload` topic of that service

### Requirement: The mapper converts a c8y_ServiceCommand operation into a thin-edge command

The c8y mapper SHALL natively handle the `c8y_ServiceCommand` fragment
received on the JSON over MQTT operation channel.
It SHALL NOT rely on a shipped custom operation handler file.

For a valid operation, the mapper SHALL publish a thin-edge command with:

- the target entity resolved from `externalSource.externalId`;
- the command topic `te/device/<device>/service/<service>/cmd/control/<cmd-id>`;
- `action` taken from the `command` value of the operation, unchanged;
- `serviceName` taken from the `serviceName` value of the operation payload,
  which is the name `tedge service` passes to the backend,
  falling back to the `name` of the service's registration data;
- `serviceType` taken from the `serviceType` value of the operation payload,
  falling back to the `type` of the service's registration data,
  and to the default type `service` when neither gives a type;
- `status` set to `init`.

#### Scenario: A restart operation becomes a control command
- **WHEN** the mapper receives a `c8y_ServiceCommand` operation with `command` `restart`,
  whose external id resolves to the `collectd` service of the main device
- **THEN** it SHALL publish `te/device/main/service/collectd/cmd/control/<cmd-id>`
  with `status` `init`, `action` `restart` and `serviceName` `collectd`

#### Scenario: The service type of the operation comes first
- **WHEN** the operation payload carries `serviceType` `systemd`
  and the target service was registered with type `container`
- **THEN** the published command SHALL carry `serviceType` `systemd`

#### Scenario: A service registered without a type
- **WHEN** the target service has no type in its registration data
  and the operation payload carries no usable `serviceType`
- **THEN** the published command SHALL carry the default type `service`

#### Scenario: The service name comes from the payload
- **WHEN** the operation payload's `serviceName` differs from the service segment
  of the resolved entity's topic identifier
- **THEN** the published command SHALL carry the name from the payload,
  on the topic of the resolved entity

### Requirement: The mapper rejects invalid c8y_ServiceCommand operations

The c8y mapper SHALL fail the cloud operation, with a reason explaining why,
and SHALL NOT publish any thin-edge command, when:

- the target entity cannot be resolved from `externalSource.externalId`;
- the `command` value is not exactly one of the actions the target service has declared;
- the `command` value does not match the action name rule `[A-Za-z][A-Za-z0-9_-]*`,
  which is what `tedge service` accepts;
- neither the operation nor the registration data gives a service name,
  or the name does not match the service name rule of `tedge service`:
  non-empty with no leading `-`.
  Checking the name here gives the operator the reason
  instead of a generic backend failure;
- the service type, whether it comes from the registration data or from the operation payload,
  does not match the service type rule of `tedge service`,
  since it names a file under the service plugin directory.

The failure SHALL be reported on the SmartREST topic of the target,
the topic Cumulocity created the operation on.
A target that cannot be resolved SHALL be addressed by the external id the operation carries,
as the service a service command always names.
Nothing SHALL be published when that external id names no valid topic.

#### Scenario: The target cannot be resolved
- **WHEN** the operation's external id matches no known entity
- **THEN** the mapper SHALL fail the operation with a reason naming the unresolvable target,
  on `c8y/s/us/<external-id>`

#### Scenario: An undeclared action is refused
- **WHEN** the operation requests an action the target service has not declared
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

#### Scenario: The action is matched with its case
- **WHEN** the operation requests `RESTART` and the service declares `restart`
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

#### Scenario: A malformed command value is refused
- **WHEN** the operation's `command` is empty, carries a space,
  or contains a character outside `[A-Za-z0-9_-]`
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

#### Scenario: A service name a backend could misread is refused
- **WHEN** neither the operation's `serviceName` nor the registered `name` is set,
  or the service name starts with `-`
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

#### Scenario: A service type naming no plugin file is refused
- **WHEN** the resolved service type holds a character outside `[a-z0-9_-]`,
  whether the service registered it or the operation carries it
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

### Requirement: The mapper reports the service command status back to Cumulocity

The c8y mapper SHALL report the state of the `control` command of a service to Cumulocity
with the existing command status mapping (SmartREST `501` to `506`),
on the SmartREST topic of the service.
The operation reported SHALL be `c8y_ServiceCommand`.

#### Scenario: A successful command is reported
- **WHEN** the service command reaches `successful`
- **THEN** the mapper SHALL mark the Cumulocity operation as successful

#### Scenario: A failed command is reported with its reason
- **WHEN** the service command reaches `failed` with a reason
- **THEN** the mapper SHALL mark the Cumulocity operation as failed and SHALL include that reason
