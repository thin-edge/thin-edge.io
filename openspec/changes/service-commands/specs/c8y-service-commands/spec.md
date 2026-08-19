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

#### Scenario: A service declaring an action becomes actionable in the cloud
- **WHEN** a service publishes its first `cmd/<action>` capability
- **THEN** the mapper SHALL declare `c8y_ServiceCommand` among that service's supported operations

#### Scenario: No operation file has to be provided
- **WHEN** a service declares its capabilities over MQTT only
- **THEN** the mapper SHALL register the supported operation on its own,
  with nothing to install beforehand

### Requirement: The mapper aggregates declared actions into c8y_SupportedServiceCommands

The c8y mapper SHALL collect all `cmd/<action>` capabilities declared by a service
and SHALL publish them as a single `c8y_SupportedServiceCommands` array
on the service's managed object, through an inventory update over the JSON over MQTT API.

Action names SHALL be uppercased, following the Cumulocity convention.
Custom action names SHALL be passed through unchanged apart from the case mapping.
The array SHALL be sorted, so that the same set of actions always gives the same array.

A capability whose action name does not match the action name rule `[a-z][a-z0-9_-]*`
SHALL NOT be declared at all, and SHALL only be logged.
Cumulocity would otherwise offer a command whose lowercased name
matches no capability topic of that service,
so the action could never be routed back to it.

The array SHALL be re-published whenever the set of declared actions changes.
When a capability topic is cleared, the mapper SHALL drop that action from the set
and publish the reduced array.
On mapper restart, the retained capability messages replay,
so the mapper SHALL rebuild the full set and SHALL NOT lose a previously declared action.

#### Scenario: Standard actions are published in uppercase
- **WHEN** a service declares `cmd/start`, `cmd/stop` and `cmd/restart`
- **THEN** the mapper SHALL set `{"c8y_SupportedServiceCommands": ["RESTART", "START", "STOP"]}`
  on that service's managed object

#### Scenario: A custom action is passed through
- **WHEN** a service declares `cmd/pause`
- **THEN** `PAUSE` SHALL appear in that service's `c8y_SupportedServiceCommands`

#### Scenario: An action name the cloud could not send back is not declared
- **WHEN** a service declares `cmd/doSomething`
- **THEN** `DOSOMETHING` SHALL NOT appear in that service's `c8y_SupportedServiceCommands`,
  and the capability SHALL be logged as dropped

#### Scenario: A withdrawn action is removed
- **WHEN** a service clears its `cmd/pause` capability topic
- **THEN** the mapper SHALL re-publish `c8y_SupportedServiceCommands` without `PAUSE`

#### Scenario: All actions are withdrawn
- **WHEN** a service clears every one of its `cmd/<action>` capability topics
- **THEN** the mapper SHALL publish an empty `c8y_SupportedServiceCommands` array,
  so that Cumulocity stops offering actions for that service

#### Scenario: The set is rebuilt after a mapper restart
- **WHEN** the mapper restarts and receives the retained capability messages again
- **THEN** it SHALL publish the same complete set of actions, losing none

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
- `serviceName` taken from the `serviceName` value of the operation payload,
  which is the name `tedge service` passes to the backend;
- `serviceType` taken from the service's registration data,
  falling back to the `serviceType` value in the operation payload,
  and to the default type `service` when the service was registered without a type;
- `status` set to `init`.

#### Scenario: A RESTART operation becomes a restart command
- **WHEN** the mapper receives a `c8y_ServiceCommand` operation with `command` `RESTART`,
  whose external id resolves to the `collectd` service of the main device
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

#### Scenario: The service name comes from the payload
- **WHEN** the operation payload's `serviceName` differs from the service segment
  of the resolved entity's topic identifier
- **THEN** the published command SHALL carry the name from the payload,
  on the topic of the resolved entity

### Requirement: The mapper rejects invalid c8y_ServiceCommand operations

The c8y mapper SHALL fail the cloud operation, with a reason explaining why,
and SHALL NOT publish any thin-edge command, when:

- the target entity cannot be resolved from `externalSource.externalId`;
- the requested action, compared case-insensitively,
  is not among the actions the target service has declared;
- the lowercased `command` value does not match the action name rule `[a-z][a-z0-9_-]*`,
  which is what `tedge service` accepts;
- the operation carries no `serviceName`,
  or one which does not match the service name rule of `tedge service`:
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

#### Scenario: The action is matched case-insensitively
- **WHEN** the operation requests `RESTART` and the service declares `cmd/restart`
- **THEN** the mapper SHALL accept it and publish a `restart` command

#### Scenario: A malformed command value is refused
- **WHEN** the operation's `command` is empty, carries a space,
  or once lowercased contains a character outside `[a-z0-9_-]`
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

#### Scenario: A service name a backend could misread is refused
- **WHEN** the operation's `serviceName` is absent or empty, or starts with `-`
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

#### Scenario: A service type naming no plugin file is refused
- **WHEN** the resolved service type holds a character outside `[a-z0-9_-]`,
  whether the service registered it or the operation carries it
- **THEN** the mapper SHALL fail the operation and SHALL NOT publish a command

### Requirement: The mapper reports the service command status back to Cumulocity

The c8y mapper SHALL report the state of a service command to Cumulocity
with the existing command status mapping (SmartREST `501` to `506`),
on the SmartREST topic of the service.

The operation reported SHALL be `c8y_ServiceCommand`,
chosen from the target being a service and not from the name of the action.
Reporting a service's `restart` as `c8y_Restart`
would name an operation Cumulocity never created.

#### Scenario: A successful command is reported
- **WHEN** the service command reaches `successful`
- **THEN** the mapper SHALL mark the Cumulocity operation as successful

#### Scenario: A failed command is reported with its reason
- **WHEN** the service command reaches `failed` with a reason
- **THEN** the mapper SHALL mark the Cumulocity operation as failed and SHALL include that reason

#### Scenario: An action named as a device operation stays a service command
- **WHEN** the `restart` command of a service reaches `executing`
- **THEN** the mapper SHALL report `c8y_ServiceCommand` as executing, and not `c8y_Restart`
