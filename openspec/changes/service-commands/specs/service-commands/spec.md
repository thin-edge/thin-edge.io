## ADDED Requirements

### Requirement: A service declares each supported action as a per-action capability

A service entity SHALL declare a supported action by publishing a retained capability message
on `te/device/<device>/service/<service>/cmd/<action>` with an empty JSON object `{}` as payload.
One topic SHALL be used per action, so that each action can have its own workflow.

`<action>` SHALL be one of the standard actions
`start`, `stop`, `restart`, `enable` and `disable`,
the five an init system defines in `system.toml`,
or any custom action name chosen by the service owner.

An action name SHALL match `[a-z][a-z0-9_-]*`:
lowercase letters, digits, `_` and `-`, starting with a letter.
MQTT topic names are case-sensitive and do accept spaces,
so this is a restriction thin-edge puts on itself, not one the protocol imposes.
A name carrying spaces or mixed case, such as `do something` or `doSomething`,
is deliberately left out of scope, and SHALL be rejected wherever it enters the system.
The names then survive every conversion on the way to the cloud and back,
and stay usable as the argument of a command line.

A declared action SHALL be withdrawn by clearing its capability topic with a retained empty message.

#### Scenario: Standard actions are declared
- **WHEN** a service publishes retained `{}` on its `cmd/start`, `cmd/stop`, `cmd/restart`, `cmd/enable` and `cmd/disable` topics
- **THEN** those five actions SHALL be the actions supported by that service

#### Scenario: A custom action is declared
- **WHEN** a service publishes retained `{}` on `te/device/main/service/nodered/cmd/pause`
- **THEN** `pause` SHALL be a supported action of the `nodered` service

#### Scenario: An action is withdrawn
- **WHEN** a service clears one of its `cmd/<action>` topics with a retained empty message
- **THEN** that action SHALL no longer be a supported action of that service

#### Scenario: Capabilities survive a restart of a consumer
- **WHEN** a component subscribes to a service's `cmd/+` topics after the capabilities were published
- **THEN** it SHALL receive the retained capability messages and rebuild the full set of supported actions

### Requirement: thin-edge's own services declare their own actions

`tedge-agent` and every mapper SHALL declare their own actions when they start,
publishing the capabilities where they publish their service registration.
Which actions are declared depends on how the service is deployed.

A service managed by an init unit of its own SHALL declare:

- `restart`, `enable` and `disable`, for `tedge-agent` and for every mapper;
- the five standard actions, for `tedge-mapper-collectd` and `tedge-mapper-local`,
  the two mappers connected to no cloud.

Neither `start` nor `stop` SHALL be declared for `tedge-agent` and the cloud mappers.
The shipped workflow always refuses `stop` for these services,
so declaring it would put a button in the cloud for an action which can only fail.
A `start` is only ever asked for while the service is stopped,
and `tedge-agent` is what executes a service command:
once it is stopped, nothing is left to run that command.

Leaving an action out of the list does not prevent it from being commanded:
a capability is a plain retained message, so anyone can publish one,
and a command can be issued on MQTT with no capability declared at all.
What refuses an action SHALL therefore be the guards of the shipped workflows.

Under `tedge run all`, the agent and the mappers are hosted in a single process.
Such a hosted service has no init unit of its own,
so an action reaching an init system would act on a unit which is not what runs.
A hosted service SHALL declare `restart` if it is `tedge-agent`, that being the one action
which reaches no init system, and SHALL declare nothing otherwise.
It SHALL also withdraw every standard action it does not declare,
a capability being retained and outliving the deployment which published it.

A capability lives as long as the registration of the service which published it.
Uninstalling the software clears neither the registration nor its capabilities;
deregistering the service SHALL clear both.

#### Scenario: The agent declares its actions at startup
- **WHEN** `tedge-agent` starts as a service of its own
- **THEN** `restart`, `enable` and `disable` SHALL be the actions it declares,
  with nothing else having been published

#### Scenario: A cloud mapper declares the same actions
- **WHEN** a mapper connected to a cloud starts as a service of its own
- **THEN** `restart`, `enable` and `disable` SHALL be the actions it declares

#### Scenario: A mapper connected to no cloud declares every action
- **WHEN** `tedge-mapper-collectd` or `tedge-mapper-local` starts as a service of its own
- **THEN** the five standard actions SHALL be the actions it declares

#### Scenario: A service under `tedge run all` declares only what it can carry out
- **WHEN** the agent and the mappers are hosted in a single process
- **THEN** `restart` SHALL be the only action declared by the agent,
  and the mappers SHALL declare no action

#### Scenario: Moving to `tedge run all` withdraws the actions of the previous deployment
- **WHEN** a device whose services declared their actions as services of their own
  is moved to a single process
- **THEN** every standard action a hosted service does not declare SHALL be withdrawn

#### Scenario: An action declared by hand is still refused
- **WHEN** `stop` is declared by hand on the capability topic of `tedge-agent`
  and a `stop` command is issued for it
- **THEN** the command SHALL be refused by the guard of the shipped workflow,
  the declaration deciding what is offered and not what is allowed

### Requirement: A service command is issued on the per-action topic

A service command SHALL be issued as a command message on
`te/device/<device>/service/<service>/cmd/<action>/<cmd-id>`,
following the standard thin-edge command state machine (`init` → `executing` → `successful` | `failed`).

The command payload SHALL carry:

- `status`: the command state
- `serviceName`: the name of the target service, as the execution backend knows it
- `serviceType`: the type of the target service, used to select the execution backend

The payload SHALL NOT repeat the action name.
The topic is the only place the action is named,
as it is for every other thin-edge command.

A command SHALL only be issued for an action the target service has declared as a capability.

#### Scenario: A restart command is issued for a service
- **WHEN** a `restart` command is issued for the `collectd` service of the main device
- **THEN** the command SHALL be published on `te/device/main/service/collectd/cmd/restart/<cmd-id>`
  with `status` `init`, `serviceName` `collectd` and the service's type as `serviceType`

#### Scenario: The action is read from the topic
- **WHEN** a workflow or an executor needs the action name of a service command
- **THEN** it SHALL take it from the command topic, since the payload does not carry it

#### Scenario: Each command has its own channel
- **WHEN** a `restart` and a `pause` command are issued for the same service
- **THEN** they SHALL be published on distinct command topics and SHALL be driven by distinct workflows

### Requirement: Workflow definitions are scoped by target entity type

A workflow definition SHALL support a `type` field naming the entity type the workflow applies to,
taking one of the entity `@type` values (`device`, `child-device`, `service`).

A workflow SHALL only be used for commands addressed to an entity of the declared type.
When `type` is absent, the workflow SHALL apply to device entities, as it does today.

The type a target is matched against SHALL be the `@type` reported for it,
except for the device the agent runs on, which SHALL be matched as `device`.
An agent running on a child device reports itself as `child-device`,
so matching it on its reported type would leave it unable to find its own workflows.

This makes an operation name reusable across entity types:
a `restart` workflow for a device and a `restart` workflow for a service are distinct workflows.

#### Scenario: A service command does not trigger the device workflow
- **WHEN** a `restart` command is addressed to a service entity
  and both a device `restart` workflow and a `restart` workflow with `type = "service"` are installed
- **THEN** only the workflow with `type = "service"` SHALL drive the command

#### Scenario: Existing device workflows are unaffected
- **WHEN** a workflow definition has no `type` field
- **THEN** it SHALL keep applying to commands addressed to the device, as before this change

#### Scenario: An agent on a child device uses its device workflows
- **WHEN** a command is addressed to the child device an agent runs on
- **THEN** it SHALL be driven by a workflow with no `type` or with `type = "device"`

### Requirement: tedge-agent is the sole executor of service commands for its own device

tedge-agent SHALL react to the commands of the services of its **own** device
and SHALL drive their command state machines with its workflow engine.

Which device a service belongs to SHALL be taken from the registration data of that service,
not from the shape of its topic, so that this holds under a custom topic scheme too.

tedge-agent SHALL NOT react to commands addressed to services of any other device.
A child device running its own tedge-agent SHALL execute the commands of its own services,
using its own configuration and its own service backends.
No agent SHALL execute a command on behalf of another device.

Because a command topic is a single shared state record,
no other component SHALL drive the state machine of a service command.
A third-party service owner SHALL integrate below the state machine,
as a process executed by tedge-agent, and never as a competing MQTT subscriber.

#### Scenario: The agent drives a command for a service of its own device
- **WHEN** a command is issued for a service of the agent's own device
- **THEN** tedge-agent SHALL drive that command through the state machine to a terminal state

#### Scenario: The agent ignores commands for services of other devices
- **WHEN** a command is issued for a service of a device other than the agent's own device
- **THEN** that agent SHALL NOT react to the command

#### Scenario: An unresolvable target leaves the command untouched
- **WHEN** the agent cannot tell whether a command's target is a service of its own device
- **THEN** it SHALL NOT act on the command and SHALL leave the command state unchanged,
  so that the command can be retried rather than half-driven

#### Scenario: No backend for the service type is a definitive failure
- **WHEN** a command is issued for a service whose type has no execution backend installed
- **THEN** the command SHALL move to `failed` with a reason saying the action is not supported
  for that type of service, and SHALL NOT be left waiting for another executor

### Requirement: Service command workflows delegate execution to the tedge service CLI

The workflows shipped for service commands SHALL execute the command by invoking
`sudo -n tedge service <action> <serviceName> --service-type <serviceType>`,
taking the action name from the command topic and the service name and type from the command payload.

A zero exit code SHALL move the command to `successful`;
a non-zero exit code SHALL move it to `failed` with the failure reason.

The workflow SHALL carry a timeout,
so that a command whose backend never returns moves to `failed`
rather than staying in progress.

#### Scenario: A successful execution completes the command
- **WHEN** the execution step of a service command workflow exits with code `0`
- **THEN** the command SHALL move to `successful`

#### Scenario: A failed execution fails the command
- **WHEN** the execution step exits with a non-zero code
- **THEN** the command SHALL move to `failed` with a reason reported to the requester

#### Scenario: A hung backend fails the command
- **WHEN** the execution step does not return within the workflow timeout
- **THEN** the command SHALL move to `failed` rather than remain in progress indefinitely

### Requirement: Service commands targeting thin-edge's own services are handled safely

A `restart` addressed to **tedge-agent** itself SHALL NOT be run as a plain synchronous step,
since the agent is killed mid-workflow and the step would re-execute on resume, causing a restart loop.
The shipped workflow SHALL use the existing self-restart pattern instead:
the agent persists the state awaiting its own restart, asks its runtime to stop,
and is started again by whatever runs it.
No backend is asked to restart the agent,
so this holds whether the agent runs as a service of its own
or is co-hosted with the mappers in a single process.

A `stop` addressed to **tedge-agent** or to a **cloud mapper** SHALL be rejected with a clear reason.
A stopped agent cannot report the completion of the command,
and a stopped mapper loses the cloud connection,
so in both cases the requester would never learn the outcome.

A mapper is recognized by its name, which is always `tedge-mapper-<x>`,
whatever its cloud, topic prefix and profile.
`tedge-mapper-collectd` and `tedge-mapper-local` are the exception, being connected to no cloud.
The requester still learns the outcome when one of them is stopped,
so both SHALL be stopped as any other service.
Neither takes a cloud profile, so these are always their exact names.

#### Scenario: Restarting the agent itself completes the command
- **WHEN** a `restart` command is issued for the tedge-agent service
- **THEN** the agent SHALL stop, resume the workflow once it is started again,
  and move the command to `successful` exactly once

#### Scenario: Stopping the agent is rejected
- **WHEN** a `stop` command is issued for the tedge-agent service
- **THEN** the command SHALL move to `failed` with a reason explaining that the executor cannot be stopped

#### Scenario: Stopping the agent named as its unit is rejected
- **WHEN** a `stop` command is issued for the tedge-agent service,
  naming it `tedge-agent.service`
- **THEN** the command SHALL move to `failed` for the same reason

#### Scenario: Stopping a cloud mapper is rejected
- **WHEN** a `stop` command is issued for a cloud mapper service
- **THEN** the command SHALL move to `failed` with a reason explaining that the cloud connection cannot be stopped this way

#### Scenario: Stopping a mapper connected to no cloud is allowed
- **WHEN** a `stop` command is issued for the `tedge-mapper-collectd`
  or the `tedge-mapper-local` service
- **THEN** the action SHALL be run as it is for any other service
