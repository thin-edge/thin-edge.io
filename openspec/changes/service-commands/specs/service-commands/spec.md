## ADDED Requirements

### Requirement: A service declares its supported actions in its `control` capability

A service entity SHALL declare the actions it supports by publishing a retained capability message
on `te/device/<device>/service/<service>/cmd/control`,
with the action names in an `actions` array:

```json
{"actions": ["start", "stop", "restart"]}
```

An action SHALL be one of the standard actions
`start`, `stop`, `restart`, `enable` and `disable`,
the five an init system defines in `system.toml`,
or any custom action name chosen by the service owner.

An action name SHALL match `[A-Za-z][A-Za-z0-9_-]*`:
letters, digits, `_` and `-`, starting with a letter.
Action names are case-sensitive, so `requestLog` and `requestlog` are two actions.
A name carrying spaces, such as `do something`,
is deliberately left out of scope, and SHALL be rejected wherever it enters the system.
The names then stay usable as the argument of a command line.

An action SHALL be removed by publishing the `control` capability again without that action.
Clearing the `control` capability topic with a retained empty message
SHALL remove every action of the service.

#### Scenario: Standard actions are declared
- **WHEN** a service publishes retained `{"actions": ["start", "stop", "restart", "enable", "disable"]}` on its `cmd/control` topic
- **THEN** those five actions SHALL be the actions supported by that service

#### Scenario: A custom action is declared
- **WHEN** a service publishes retained `{"actions": ["pause"]}` on `te/device/main/service/nodered/cmd/control`
- **THEN** `pause` SHALL be a supported action of the `nodered` service

#### Scenario: An action name keeps its case
- **WHEN** a service declares `{"actions": ["requestLog"]}`
- **THEN** `requestLog` SHALL be a supported action of that service, and `requestlog` SHALL NOT

#### Scenario: All actions are removed
- **WHEN** a service clears its `cmd/control` topic with a retained empty message
- **THEN** that service SHALL have no supported action

### Requirement: thin-edge's own services declare their own actions

`tedge-agent` and every mapper SHALL declare their own actions when they start,
publishing their `control` capability where they publish their service registration.
Which actions are declared depends on how the service is deployed.

A service managed by an init unit of its own SHALL declare `restart` and nothing else.

`restart` is the only action the cloud is given over `tedge-agent`.
A mapper declares the same one:
each mapper has a `mapper.toml` of its own,
which is where the actions it supports are to be decided, one mapper at a time.

Under `tedge run all`, the agent and the mappers are hosted in a single process.
Such a hosted service has no init unit of its own,
so an action reaching an init system would act on a unit which is not what runs.
A hosted service SHALL declare `restart` if it is `tedge-agent`, that being the one action
which reaches no init system, and SHALL declare nothing otherwise.
A hosted mapper SHALL clear the `control` capability it declares as a service of its own,
a capability being retained and outliving the deployment which published it.

A capability lives as long as the registration of the service which published it.
Uninstalling the software clears neither the registration nor its capabilities;
deregistering the service SHALL clear both.

#### Scenario: The agent declares its action at startup
- **WHEN** `tedge-agent` starts as a service of its own
- **THEN** it SHALL publish `{"actions": ["restart"]}` on its `cmd/control` topic

#### Scenario: A mapper declares the same action
- **WHEN** a mapper starts as a service of its own
- **THEN** `restart` SHALL be the only action it declares

#### Scenario: A service under `tedge run all` declares only what it can carry out
- **WHEN** the agent and the mappers are hosted in a single process
- **THEN** `restart` SHALL be the only action declared by the agent,
  and the mappers SHALL declare no action

#### Scenario: Moving to `tedge run all` clears the action of the previous deployment
- **WHEN** a device whose services declared their action as services of their own
  is moved to a single process
- **THEN** the `control` capability of every hosted mapper SHALL be cleared

### Requirement: A service command is issued as a `control` command

A service command SHALL be issued as a command message on
`te/device/<device>/service/<service>/cmd/control/<cmd-id>`,
following the standard thin-edge command state machine (`init` → `executing` → `successful` | `failed`).

The command payload SHALL carry:

- `status`: the command state
- `action`: the action to run
- `serviceName`: the name of the target service, as the execution backend knows it
- `serviceType`: the type of the target service, used to select the execution backend

A command SHALL only be issued for an action the target service has declared in its `control` capability.

#### Scenario: A restart is requested for a service
- **WHEN** the `restart` action is requested for the `collectd` service of the main device
- **THEN** the command SHALL be published on `te/device/main/service/collectd/cmd/control/<cmd-id>`
  with `status` `init`, `action` `restart`, `serviceName` `collectd`
  and the service's type as `serviceType`

#### Scenario: The action is read from the payload
- **WHEN** a workflow or an executor needs the action name of a service command
- **THEN** it SHALL take it from the `action` field of the command payload

#### Scenario: All actions of a service share one channel
- **WHEN** a `restart` and a `pause` action are requested for the same service
- **THEN** both commands SHALL be published on the `cmd/control` topic of that service
  with distinct command ids, and SHALL be driven by the same workflow

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
a `log_upload` workflow for a device and a `log_upload` workflow for a service are distinct workflows.

#### Scenario: A command for a service does not trigger the device workflow
- **WHEN** a `log_upload` command is addressed to a service entity
  and both a device `log_upload` workflow and a `log_upload` workflow with `type = "service"` are installed
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

The workflow shipped for the `control` command of services SHALL execute the command by invoking
`tedge service <action> <serviceName> --type <serviceType>` from a script state with `sudo = true`,
so that it is run through `sudo -n` when `sudo.enable` is set and sudo is installed,
taking the action name, the service name and the service type from the command payload.
This one workflow SHALL run every action, custom actions included.

A zero exit code SHALL move the command to `successful`;
a non-zero exit code SHALL move it to `failed` with the failure reason.

The workflow SHALL carry a timeout,
so that a command whose backend never returns moves to `failed`
rather than staying in progress.

#### Scenario: A custom action needs no workflow of its own
- **WHEN** the `pause` action is requested for a service
- **THEN** the shipped workflow SHALL run `tedge service pause <serviceName> --type <serviceType>`

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

A `restart` action addressed to **tedge-agent** itself SHALL NOT be run as a plain synchronous step,
since the agent is killed mid-workflow and the step would re-execute on resume, causing a restart loop.
The shipped workflow SHALL use the existing self-restart pattern instead:
the agent persists the state awaiting its own restart, asks its runtime to stop,
and is started again by whatever runs it.
No backend is asked to restart the agent,
so this holds whether the agent runs as a service of its own
or is co-hosted with the mappers in a single process.

#### Scenario: Restarting the agent itself completes the command
- **WHEN** the `restart` action is requested for the tedge-agent service
- **THEN** the agent SHALL stop, resume the workflow once it is started again,
  and move the command to `successful` exactly once
