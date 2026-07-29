## ADDED Requirements

### Requirement: The mapper registers c8y_ServiceCommand as a supported operation of a service

When the c8y mapper sees a service declaring at least one `cmd/<action>` capability,
it SHALL register `c8y_ServiceCommand` as a supported operation of that service,
using the same supported-operation mechanism it uses today (SmartREST `114`).

This registration SHALL NOT require an operation file
to be provided by the service or by an administrator:
the mapper SHALL create the supported operation entry itself
under `/etc/tedge/operations/c8y/<service-external-id>/`
and SHALL reload that service's operations,
since the operation directory of a service is not watched for changes.

#### Scenario: A service declaring a command becomes actionable in the cloud
- **WHEN** a service publishes its first `cmd/<action>` capability
- **THEN** the mapper SHALL declare `c8y_ServiceCommand` among that service's supported operations

#### Scenario: No operation file has to be provided
- **WHEN** a service declares its capabilities over MQTT only
- **THEN** the mapper SHALL register the supported operation on its own,
  with nothing to install beforehand

### Requirement: The mapper aggregates declared commands into c8y_SupportedServiceCommands

The c8y mapper SHALL collect all `cmd/<action>` capabilities declared by a service
and SHALL publish them as a single `c8y_SupportedServiceCommands` array
on the service's managed object, through an inventory update over the JSON over MQTT API.

Command names SHALL be uppercased, following the Cumulocity convention.
Custom command names SHALL be passed through unchanged apart from the case mapping.
The array SHALL be sorted, so that the same set of commands always gives the same array.

The array SHALL be re-published whenever the set of declared commands changes.
When a capability topic is cleared, the mapper SHALL drop that command from the set
and publish the reduced array.
On mapper restart, the retained capability messages replay,
so the mapper SHALL rebuild the full set and SHALL NOT lose a previously declared command.

#### Scenario: Standard commands are published in uppercase
- **WHEN** a service declares `cmd/start`, `cmd/stop` and `cmd/restart`
- **THEN** the mapper SHALL set `{"c8y_SupportedServiceCommands": ["RESTART", "START", "STOP"]}`
  on that service's managed object

#### Scenario: A custom command is passed through
- **WHEN** a service declares `cmd/pause`
- **THEN** `PAUSE` SHALL appear in that service's `c8y_SupportedServiceCommands`

#### Scenario: A withdrawn command is removed
- **WHEN** a service clears its `cmd/pause` capability topic
- **THEN** the mapper SHALL re-publish `c8y_SupportedServiceCommands` without `PAUSE`

#### Scenario: All commands are withdrawn
- **WHEN** a service clears every one of its `cmd/<action>` capability topics
- **THEN** the mapper SHALL publish an empty `c8y_SupportedServiceCommands` array,
  so that Cumulocity stops offering actions for that service

#### Scenario: The set is rebuilt after a mapper restart
- **WHEN** the mapper restarts and receives the retained capability messages again
- **THEN** it SHALL publish the same complete set of commands, losing none

### Requirement: The mapper converts a c8y_ServiceCommand operation into a thin-edge command

The c8y mapper SHALL natively handle the `c8y_ServiceCommand` fragment
received on the JSON over MQTT operation channel.
It SHALL NOT rely on a shipped custom operation handler file.

For a valid operation, the mapper SHALL publish a thin-edge command with:

- the target entity resolved from `externalSource.externalId`;
- the command topic built from the lowercased `command` value,
  as `te/device/<device>/service/<service>/cmd/<action>/<cmd-id>`.
  Cumulocity uppercases a command name by convention,
  so lowercasing the value it sends gives back the action name the service declared.
  The action is named by the topic only;
  the mapper SHALL NOT copy the cloud's `command` value into the thin-edge payload;
- `serviceName` derived from the resolved entity topic identifier, not from the cloud payload,
  since the cloud value may be a display name.
  A custom topic scheme carries no service name in the topic identifier,
  in which case the name given at registration is used;
- `serviceType` taken from the service's registration data,
  falling back to the `serviceType` value in the operation payload,
  and to the default type `service` when the service was registered without a type;
- `status` set to `init`.

#### Scenario: A RESTART operation becomes a restart command
- **WHEN** the mapper receives a `c8y_ServiceCommand` operation with `command` `RESTART`
  for the external id `<device-id>:device:main:service:collectd`
- **THEN** it SHALL publish `te/device/main/service/collectd/cmd/restart/<cmd-id>`
  with `status` `init` and `serviceName` `collectd`, and no action name in the payload

#### Scenario: The service type comes from the registration
- **WHEN** the target service was registered with type `container`
- **THEN** the published command SHALL carry `serviceType` `container`,
  whatever the operation payload says

#### Scenario: A service registered without a type
- **WHEN** the target service has no type in its registration data
  and the operation payload carries no usable `serviceType`
- **THEN** the published command SHALL carry the default type `service`

#### Scenario: The service name comes from the entity, not the payload
- **WHEN** the operation payload's `serviceName` is a display name differing from the entity name
- **THEN** the published command SHALL carry the name derived from the resolved entity topic identifier

### Requirement: The mapper rejects invalid c8y_ServiceCommand operations

The c8y mapper SHALL fail the cloud operation, with a reason explaining why,
and SHALL NOT publish any thin-edge command, when:

- the target entity cannot be resolved from `externalSource.externalId`;
- the requested command, compared case-insensitively,
  is not among the commands the target service has declared;
- the lowercased `command` value does not match the action name rule `[a-z][a-z0-9_]+`,
  which is what `tedge service` accepts.

The failure SHALL be reported on the SmartREST topic of the target service,
or on the topic of the main device when the target itself cannot be resolved.

#### Scenario: The target cannot be resolved
- **WHEN** the operation's external id matches no known entity
- **THEN** the mapper SHALL fail the operation with a reason naming the unresolvable target

#### Scenario: An undeclared command is refused
- **WHEN** the operation requests a command the target service has not declared
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

#### Scenario: The command is matched case-insensitively
- **WHEN** the operation requests `RESTART` and the service declares `cmd/restart`
- **THEN** the mapper SHALL accept it and publish a `restart` command

#### Scenario: A malformed command value is refused
- **WHEN** the operation's `command` is empty, carries a space,
  or once lowercased contains a character outside `[a-z0-9_]`
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

### Requirement: The mapper reports the service command status back to Cumulocity

The c8y mapper SHALL report the state of a service command to Cumulocity
with the existing command status mapping (SmartREST `501` to `506`),
on the SmartREST topic of the service.

The operation reported SHALL be `c8y_ServiceCommand`,
decided by the target being a service and not by the name of the action.
A service action is not the device operation its name would otherwise map to:
reporting the status of the `restart` command of a service as `c8y_Restart`
would name an operation that Cumulocity never created.

#### Scenario: A successful command is reported
- **WHEN** the service command reaches `successful`
- **THEN** the mapper SHALL mark the Cumulocity operation as successful

#### Scenario: A failed command is reported with its reason
- **WHEN** the service command reaches `failed` with a reason
- **THEN** the mapper SHALL mark the Cumulocity operation as failed and SHALL include that reason

#### Scenario: An action named as a device operation stays a service command
- **WHEN** the `restart` command of a service reaches `executing`
- **THEN** the mapper SHALL report `c8y_ServiceCommand` as executing, and not `c8y_Restart`
