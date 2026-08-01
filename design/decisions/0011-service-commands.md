# Service command support (Cumulocity service commands)

* Date: __2026-07-23__
* Status: __Approved__

## Background

Cumulocity provides an action interface for services.
To use it, a service must fulfil two conditions on the Cumulocity side:

1. `c8y_ServiceCommand` is listed in the service's `c8y_SupportedOperations` fragment.
2. the service's managed object contains the `c8y_SupportedServiceCommands` fragment
   (e.g. `["START", "STOP", "RESTART"]`) listing the commands it supports.

The UI then shows action buttons on that service.
Pressing one creates an operation carrying the `c8y_ServiceCommand` fragment,
addressed to the service's external id:

```json
{
  "c8y_ServiceCommand": {
    "serviceType": "service",
    "serviceName": "tedge-mapper-c8y",
    "command": "RESTART"
  },
  "externalSource": {
    "externalId": "<device-id>:device:main:service:tedge-mapper-c8y",
    "type": "c8y_Serial"
  }
}
```

thin-edge.io already models services as first-class entities (`te/device/<device>/service/<service>`)
with registration, twin data, telemetry and health monitoring.
However, acting on a service is not really supported today:

* **Capability declaration**: a user *can* declare `c8y_ServiceCommand` as a supported operation for a service
  by manually adding an operation file under `/etc/tedge/operations/c8y/<service-external-id>/`.
  However, unlike for the main device,
  changes in child-device and service operation directories are not tracked by inotify,
  so the file is not picked up dynamically.
  In addition, the `c8y_SupportedServiceCommands` fragment still has to be set on the service's managed object by hand.
* **Execution**: tedge-agent's workflow engine currently does not react to *any* commands addressed to service entities,
  so there is no component that would carry out an operation targeting a service.

Supporting this is harder than a plain mapper feature
because **services on one device are controlled through different mechanisms**:

1. **Init-managed services** (systemd, OpenRC, SysVinit, ...):
   thin-edge's own daemons (`tedge-agent`, `tedge-mapper-*`) and arbitrary units.
   thin-edge already abstracts the init system behind `/etc/tedge/system.toml`.
2. **Services managed by a third-party daemon**:
   e.g. containers registered as services by
   [tedge-container-plugin](https://github.com/thin-edge/tedge-container-plugin)
   with service types `container` / `container-group`.
   Only that daemon knows how to start/stop/restart them.

Converting messages between the Cumulocity format and the thin-edge format
is largely a matter of reusing existing mapper mechanisms.
The open design question, and the core of this proposal,
is **who executes a service command, and how**.

## Goals

* A **cloud-agnostic** thin-edge interface:
  services declare which actions they support and receive them on the standard `te/.../cmd/...` topics.
  The Cumulocity specifics live entirely in the c8y mapper.
* Support the standard actions `start`, `stop`, `restart`, plus **custom actions**:
  Cumulocity allows arbitrary command names in `c8y_SupportedServiceCommands`,
  and a service owner can define its own.
* Works both for init-managed services (abstracted by `system.toml`)
  and for services controlled through a third-party abstraction (e.g. containers via `tedge-container-plugin`),
  without conflicting executors.
* No broadening of the privileged surface beyond what thin-edge packaging already grants.

## Design

At a glance, four roles are involved:

* **Service owner** (tedge-agent, a mapper, or a third-party daemon like `tedge-container-plugin`):
  registers the service as an entity and is the source of its supported actions.
* **tedge-mapper-c8y**: converts between Cumulocity and thin-edge
    * the capability → `c8y_SupportedServiceCommands` + supported operation (SmartREST `114`)
    * a `c8y_ServiceCommand` operation → a thin-edge command;
    * the command status → SmartREST `501`–`506`.
* **tedge-agent**: executes the command with its workflow engine,
  delegating to the new `tedge service` CLI.
* **Service plugin** (per service type, optional):
  executes the actions of services not managed by the init system (e.g. containers).

### thin-edge interface

#### Command shape: one command per action (`cmd/start`, `cmd/stop`, `cmd/restart`, `cmd/<custom>`)

A service command is addressed as below:

* Capability: `te/device/<device>/service/<service>/cmd/<start|stop|restart|custom>`
  ```json
  {}
  ```
* Command: `te/device/<device>/service/<service>/cmd/restart/<cmd-id>`
  ```json
  {
    "status": "init",
    "serviceName": "tedge-mapper-c8y",
    "serviceType": "service"
  }
  ```
* This is arguably the more natural thin-edge API, and allows a distinct workflow per action.
* It needs the workflow engine to scope workflows by entity type,
  so that a service `restart` does not also trigger the device reboot workflow.
  -> Add `type = "service"` filter in the workflow definition.
* (c8y) The c8y mapper has to aggregate an open set of `cmd/<action>` topics into one `c8y_SupportedServiceCommands` list.

An action name must match `[a-z][a-z0-9_]+`:
lowercase letters, digits and `_`, starting with a letter.
MQTT topic names are case-sensitive and do accept spaces,
so this is a restriction thin-edge puts on itself, not one the protocol imposes.
A name with spaces or mixed case, such as `do something` or `doSomething`,
is out of scope and is rejected wherever it enters the system.

##### JSON field
* `status`: the states used by workflow
* `serviceName`: the service's name parsed from cloud operation (e.g. `tedge-mapper-c8y`)
* `serviceType`: the service's type. Key to select a right service plugin (e.g. `service`, `container`)

##### (c8y) Aggregation of capabilities

* On mapper restart, the retained `cmd/<action>` messages replay,
  so the mapper rebuilds the full set and no capability is lost.
* A capability is removed when its `cmd/<action>` topic is cleared (retained empty message).
  The mapper drops it from the set and re-publishes the reduced `c8y_SupportedServiceCommands` array.
* Withdrawing every action leaves an empty array.
  `c8y_ServiceCommand` stays a registered supported operation of the service,
  so Cumulocity still offers the operation, with no action to pick.

#### Capability declaration: Who publishes it?

There are two options: push-model and pull-model.
* **Push-model**: The service owner publishes their capabilities.
* **Pull-model**: `tedge-agent` discovers the capabilities of the supported service types and publishes them.

For the first iteration, push-model is selected as the scope is limited to already registered service entities like `tedge-agent`.
The pull-model design is described in the Future Consideration section.


### Cumulocity mapping

This part deliberately reuses existing mapper mechanisms; no new concepts are introduced.

* **Capability → Cumulocity.**
  When the mapper receives `te/device/<device>/service/<service>/cmd/<action>` capabilities from a service entity, it:
  1. registers `c8y_ServiceCommand` as a supported operation for that service,
     reusing the same mechanism used today for other supported operations.
  2. aggregates all capabilities and sets them as `{"c8y_SupportedServiceCommands": ["START", "STOP", ...]}`
     (uppercase, following the Cumulocity convention) on the service's managed object,
     by publishing an inventory update through the JSON over MQTT API.
     Since Cumulocity accepts arbitrary command names,
     custom names are passed through unchanged apart from the case mapping.
* **Every capability of a service is a service action.**
  On a service entity, each `cmd/<action>` capability becomes an entry of `c8y_SupportedServiceCommands`,
  so the built-in per-operation mappings (`c8y_Restart`, `c8y_LogfileRequest`, `c8y_UploadConfigFile`, ...)
  no longer apply to a service.
  A service declaring `cmd/restart` gets `RESTART` in `c8y_SupportedServiceCommands`, not `c8y_Restart`.
  Nothing changes for a device.
* **An action name breaking the rule is not declared at all.**
  Such a capability is only logged, and dropped.
  Cumulocity would otherwise show a command whose lowercased name matches no capability topic,
  so it could never be routed back to the service.
* **Operation → command.**
  The mapper natively understands the `c8y_ServiceCommand` fragment from the JSON-over-MQTT operation channel
  (first-class, *not* a shipped custom operation handler file):
  * the target entity is resolved from `externalSource.externalId`.
    Unresolvable targets fail the cloud operation with a clear reason.
  * `command` is validated case-insensitively against the declared capabilities.
    An undeclared action fails the cloud operation.
    In addition, the action name must pass the single-token validation
    described under Security considerations;
  * the command topic is built from the lowercased `command` value.
    The action is named by the topic only:
    the cloud's `command` value is not copied into the thin-edge payload;
  * `serviceName` is derived from the resolved entity topic id, not from the cloud payload
    (the cloud value may be a display name);
  * `serviceType` is taken from the service's registration data (its `type` property),
    falling back to the payload value.
    Declaring the service type at registration is optional today,
    but effectively becomes mandatory for services using this feature:
    a service registered without a type is dispatched as the default type `service` (init-managed).
* **Status → Cumulocity.**
  * The existing command-status mapping (SmartREST `501`–`506`) and the service's own SmartREST topic are reused.

### Execution: central executor with pluggable dispatch

**Exactly one executor per device**: the tedge-agent workflow engine,
extended to also react to commands addressed to *services of its own device*
(today it only reacts to commands addressed to the device itself).

**Opening up the workflow engine, deliberately narrow.**
The scope is not expressed by the subscription.
tedge-agent subscribes to the commands of every entity and decides per message
whether the target is a service of its own device:

* the target is resolved through the REST API of the entity store,
  which is where the device–service relation is kept.
  A topic filter would work only with the default topic scheme
  (`te/device/<device>/service/<service>`), where the topic itself names the parent device.
* the one exception is the device the agent runs on:
  it is recognized by comparing topic identifiers, and needs no lookup.
* when the lookup fails, the command is left untouched, so that it can be retried
  rather than half-driven.
* Introduce a `type` field in the workflow definition to avoid name collision between different targets.
  (e.g. `restart` for a device and a service must have a different workflow.)
  ```toml
  operation = "restart"
  type = "service" # Can take one of the `@type` values: <device|child-device|service>
  ```
  The device an agent runs on is matched as `device`, whatever `@type` the entity store reports for it.
  An agent running on a child device reports itself as `child-device`,
  and matching it on its reported type would leave it unable to find its own workflows.
  So `child-device` matches nothing today.
  It is reserved for the future case of an agent driving the workflows of another device.
* The file name of a workflow is free.
  The operation name comes from the `operation` field parsed from the definition
  (`load_operation_workflow` in `crates/core/tedge_agent/src/operation_workflows/persist.rs`),
  never from the file name, so a device `restart` and a service `restart`
  live in the same directory under any two names.

The model is per-device.
A child device running its own tedge-agent executes the actions for its own services,
using its own `system.toml`, its own `tedge` CLI/sudoers rule and its own service plugins.
No agent executes a command on behalf of another device.
Lookups do cross devices: an agent on a child device queries the entity store of the main device
over HTTP to learn the parent of a service.

### New CLI command `tedge service` and service plugin

The workflow's execution step delegates to a new CLI command:

```
sudo -n tedge service <action> <service-name> [--service-type <type>]
```

where `<action>` is a validated single token, e.g. `start`, `stop` or `restart`.
Which actions are supported is decided by the backend,
as `tedge service` dispatches on the service type:

| service type          | backend                                                                 |
| --------------------- | ----------------------------------------------------------------------- |
| `service` (default)   | built-in init-system abstraction (`/etc/tedge/system.toml`): an action is supported if `system.toml` defines an action template for it; the standard actions out of the box, and custom actions (e.g. `reload`) by adding an entry to `system.toml` |
| any other type        | **service plugin**: `/usr/share/tedge/service-plugins/<type> <action> <name>`; the action is passed through, the plugin decides what it supports |
| unsupported action / no plugin for the type | `tedge service` fails with a distinct exit code and a clear error message |

The exit codes are:

* `0`: the action succeeded;
* `2`: the action is not supported for this service type,
  because `system.toml` defines no template for it or no plugin is installed for the type;
* any other non-zero code: the action failed, with the reason on stderr.

##### Custom action templates in `system.toml`

A custom action template is a plain key of the `[init]` table,
with the same form as the templates already there:
an argv list with a `{}` placeholder for the service name.
`[init]` therefore stops rejecting unknown keys: `deny_unknown_fields` is dropped.

Two things keep a misspelled key discoverable:
the actions read from `[init]` are logged when the configuration is loaded,
and `tedge service` lists the known actions when it rejects an unsupported one.

The keys of `[init]` that are not service actions — the init system name
and the templates that query state rather than act on a service — stay reserved,
and are not dispatchable as actions.

#### Service plugin contract

* an executable at `/usr/share/tedge/service-plugins/<service-type>`.
* invoked as `<plugin> <action> <service-name>`,
  where `<action>` is `start`, `stop`, `restart` or a plugin-defined custom action.
* exit `0` on success;
  `2`, reserved for this meaning, when the action is not supported by this plugin;
  any other non-zero code on failure, with the reason on stderr.
* an action a plugin defines is named within the `[a-z][a-z0-9_]+` rule.
  The plugin receives the name exactly as the service declared it,
  and does not have to accept another spelling of it.

#### Examples
* `sudo tedge service restart collectd --service-type service`
-> executes `sudo systemctl restart collectd` (`service` type mapped to `system.toml` init system definition)
* `sudo tedge service restart nodered --service-type container`
-> executes `sudo /usr/share/tedge/service-plugins/container restart nodered` (`container` type mapped to a service plugin)


#### Why this shape:

* **The command protocol allows only one state-machine driver per command topic anyway.**
  A command topic is a shared, retained state record:
  if two components reacted to the same command
  (e.g. the agent's workflow engine *and* an owner daemon),
  they would publish conflicting state transitions on the same topic and corrupt each other,
  independently of who is "right".
  The agent does filter per message, so it could drive a subset.
  It deliberately claims *every* service of its own device instead,
  because a shared state record has to have exactly one driver:
  **tedge-agent is the sole driver for its device's services,
  and third parties integrate *below* the state machine (an executed plugin process),
  never *beside* it (a competing MQTT subscriber)**.
* **Single executor ⇒ no terminal-state hazard.**
  Since nothing else will ever execute the command,
  failing with "no handler installed for service type `<type>`" is *correct*,
  not a race against a real handler.
* The `tedge service` command is independently useful for operators:
  an init-system-agnostic wrapper (`tedge service restart mosquitto`) that honors `system.toml`.
  Today this requires knowing which init system the device uses.
* It resolves two practical execution problems at once:
  workflow scripts cannot read `system.toml`, and service actions need root.
  `tedge` under `sudo -n` is already authorized by the shipped sudoers configuration,
  so no packaging change is needed.

**Self-targeting services** (the executor acting on itself or its peers):

* `restart` of **tedge-agent** itself must not run as a plain synchronous step:
  the agent is killed mid-workflow and, on resume, the step would re-execute: a restart loop.
  The shipped workflow uses the `restart-agent` action instead:
  the agent persists the state awaiting its restart and asks its runtime to stop.
  No backend is asked to restart it.
  This holds whether the agent runs as a service of its own,
  or is co-hosted with the mappers under `tedge run all`, where no `tedge-agent` unit exists.
* `stop` of **tedge-agent** (the executor) and of any **cloud mapper** (e.g. tedge-mapper-c8y)
  is rejected with a clear failure reason.
  A stopped executor cannot report completion, and a stopped mapper loses the cloud connection,
  so the operation would hang in the cloud forever.
  A mapper is recognized by its name, always `tedge-mapper-<x>`,
  whatever its cloud, topic prefix and profile.
  `tedge-mapper-collectd` and `tedge-mapper-local` are the exception, being connected to no cloud.
  Stopping one of them takes no way of reporting anything away,
  so both are stopped as any other service.
  Neither takes a cloud profile, so both names are fixed.

The workflow carries a timeout so that a hung backend surfaces as a clean failure rather than a stuck operation.

## End-to-end examples

All examples assume the main device with the default topic scheme.

### A systemd service with the standard actions

`collectd` declares its own capabilities at startup:

| Topic | Payload |
|---|---|
| `te/device/main/service/collectd/cmd/start` | `{}` |
| `te/device/main/service/collectd/cmd/stop` | `{}` |
| `te/device/main/service/collectd/cmd/restart` | `{}` |
| `te/device/main/service/collectd/cmd/enable` | `{}` |
| `te/device/main/service/collectd/cmd/disable` | `{}` |

The c8y mapper reacts to this capability message:

* registers `c8y_ServiceCommand` as a supported operation of the service (SmartREST `114`);
* sets `{"c8y_SupportedServiceCommands": ["START", "STOP", "RESTART", "ENABLE", "DISABLE"]}`
  on the service's managed object.

A user creates an operation with the RESTART button in Cumulocity,
and the mapper receives the `c8y_ServiceCommand` operation:

```json
{
  "c8y_ServiceCommand": {
    "serviceType": "service",
    "serviceName": "collectd",
    "command": "RESTART"
  },
  "externalSource": {
    "externalId": "<device-id>:device:main:service:collectd",
    "type": "c8y_Serial"
  }
}
```

It resolves the target entity from the external id,
validates `restart` against the declared capabilities,
and publishes the command:

Topic: `te/device/main/service/collectd/cmd/restart/c8y-mapper-123`
```json
{
  "status": "init",
  "serviceName": "collectd",
  "serviceType": "service"
}
```

Then the `restart` workflow of tedge-agent drives the command.
This is the definition shipped by tedge-agent,
which creates the file if it is missing and never overwrites a file the user has changed.

File: `/etc/tedge/operations/service_restart.toml`
```toml
operation = "restart"
type = "service"

[init]
action = "proceed"
on_success = "executing"

[executing]
action = "proceed"
on_success = "evaluate-agent-restart"

[evaluate-agent-restart]
script = "test ${.payload.serviceName} = tedge-agent"
on_exit.0 = "restart-agent"
on_exit.1-255 = "execute"

[restart-agent]
action = "restart-agent"
on_exec = "await-agent-restart"

[await-agent-restart]
action = "await-agent-restart"
timeout_second = 90
on_success = "successful"
on_timeout = { status = "failed", reason = "tedge-agent did not restart in time" }

[execute]
script = "sudo -n tedge service ${.topic.operation} ${.payload.serviceName} --service-type ${.payload.serviceType}"
# systemd waits for the service to stop and to start again, 90 seconds each by default
timeout_second = 180
on_exit.0 = "successful"
# A reason has to be given per exit code: the reason given by the backend goes to the operation
# log, and only what a script prints on stdout can update the state of a command
on_exit.1 = { status = "failed", reason = "The action failed, see the operation log for the reason given by the backend" }
on_exit.2 = { status = "failed", reason = "This action is not supported for that type of service" }
on_exit.3-255 = { status = "failed", reason = "The action failed, see the operation log for the reason given by the backend" }
on_kill = { status = "failed", reason = "The action did not complete in time and was cancelled" }

[successful]
action = "cleanup"

[failed]
action = "cleanup"
```

* The `execute` script resolves to `sudo -n tedge service restart collectd --service-type service`.
* Then type `service` routes to `systemctl restart collectd`.
* Exit code `0` → status `successful` → the mapper reports `506`.
* A reason is declared per exit code, not once for every failure.
  A wildcard `on_error = { status = "failed", reason = "..." }` does not do what it looks like:
  the engine replaces the reason of a wildcard handler by `<program> returned exit code <n>`.

### A container service with standard and custom actions

tedge-container-plugin registers `nodered` with type `container` and declares:

| Topic | Payload |
|---|---|
| `te/device/main/service/nodered/cmd/start` | `{}` |
| `te/device/main/service/nodered/cmd/stop` | `{}` |
| `te/device/main/service/nodered/cmd/restart` | `{}` |
| `te/device/main/service/nodered/cmd/pause` | `{}` |
| `te/device/main/service/nodered/cmd/unpause` | `{}` |

The c8y mapper reacts to this capability message:

* registers `c8y_ServiceCommand` as a supported operation of the service (SmartREST `114`);
* sets `{"c8y_SupportedServiceCommands": ["START", "STOP", "RESTART", "PAUSE", "UNPAUSE"]}`
  on the service's managed object.

A user creates an operation with the PAUSE button in Cumulocity,
and the mapper receives the `c8y_ServiceCommand` operation:

```json
{
  "c8y_ServiceCommand": {
    "serviceType": "container",
    "serviceName": "nodered",
    "command": "PAUSE"
  },
  "externalSource": {
    "externalId": "<device-id>:device:main:service:nodered",
    "type": "c8y_Serial"
  }
}
```

A PAUSE operation becomes:

Topic: `te/device/main/service/nodered/cmd/pause/c8y-mapper-124`

```json
{
  "status": "init",
  "serviceName": "nodered",
  "serviceType": "container"
}
```

A custom action has no shipped workflow.
The user writes one, with the same execution step as the shipped ones:

File: `/etc/tedge/operations/service_pause.toml`
```toml
operation = "pause"
type = "service"

[init]
action = "proceed"
on_success = "executing"

[executing]
action = "proceed"
on_success = "execute"

[execute]
script = "sudo -n tedge service ${.topic.operation} ${.payload.serviceName} --service-type ${.payload.serviceType}"
timeout_second = 180
on_exit.0 = "successful"
on_exit.1 = { status = "failed", reason = "The action failed, see the operation log for the reason given by the backend" }
on_exit.2 = { status = "failed", reason = "This action is not supported for that type of service" }
on_exit.3-255 = { status = "failed", reason = "The action failed, see the operation log for the reason given by the backend" }
on_kill = { status = "failed", reason = "The action did not complete in time and was cancelled" }

[successful]
action = "cleanup"

[failed]
action = "cleanup"
```

* The `execute` script resolves to `sudo -n tedge service pause nodered --service-type container`,
which executes `/usr/share/tedge/service-plugins/container pause nodered`.
* The plugin maps the action to its container engine (e.g. `docker pause`).

Example service plugin script:
```sh
#!/bin/sh
# Service plugin for the "container" service type.
# Installed as: /usr/share/tedge/service-plugins/container
# Invoked by `tedge service` as: container <action> <service-name>
#
# Exit codes:
#   0   success
#   1   action failed (reason on stderr)
#   2   action not supported by this plugin (reserved)
set -eu

ACTION="$1"
NAME="$2"

case "$ACTION" in
    start)   docker start   "$NAME" ;;
    stop)    docker stop    "$NAME" ;;
    restart) docker restart "$NAME" ;;
    pause)   docker pause   "$NAME" ;;
    unpause) docker unpause "$NAME" ;;
    *)
        echo "container plugin: unsupported action '$ACTION'" >&2
        exit 2
        ;;
esac
```

## Alternative considered

### Alternative for command shape: one-command

Using one command channel `cmd/service_command` for all kinds of service commands.
This is not selected because it forces one workflow to address all commands,
which is against the thin-edge's topic/workflow design.

Capability: `te/device/<device>/service/<service>/cmd/service_command`.
```json
{
  "commands": ["start", "restart", "stop", "custom"]
}
```
Command: `te/device/<device>/service/<service>/cmd/service_command/<cmd-id>`
```json
{
  "status": "init",
  "command": "restart",
  "serviceName": "tedge-mapper-c8y",
  "serviceType": "service"
}
```

* All supported commands are included in the service's metadata payload.
* A single workflow handles every service command in `service_command.toml`.
* The single command channel serializes commands per service:
  only one service command can run at a time.
* (c8y) The c8y mapper copies the declared commands straight into `c8y_SupportedServiceCommands`.

### Alternative for executor: no workflow

Each service owner executes the commands for its own services:
* init-managed services: tedge-agent handles.
* other services: a custom daemon subscribes to `.../cmd/<command>/+` for its services
and drives the `init → executing → successful|failed` state machine itself.

This is not selected, because:
* Like other commands, tedge-agent should be the central executor to manage all states.
* A custom service plugin can cover the flexibility of custom operation requirements.


### Alternative for the c8y mapper part: custom operation handler files

Instead of native mapper support, using a custom operation handler file.
Since a service has to declare not only `c8y_SupportedOperations`, 
but also `c8y_SupportedServiceCommands`, this model makes it difficult.

Also, note that dynamic reloading of custom operation handlers files of child devices/services is disabled.

```toml
[exec]
topic = "c8y/devicecontrol/notifications"
on_fragment = "c8y_ServiceCommand"

[exec.workflow]
operation = "${.payload.c8y_ServiceCommand.command}"
input.serviceType = "${.payload.c8y_ServiceCommand.serviceType}"
input.serviceName = "${.payload.c8y_ServiceCommand.serviceName}"
```

## Security considerations

* **The privileged surface is `tedge service`.**
  It runs as root via the already-shipped sudoers rule for `/usr/bin/tedge`.
  Consequently:
  * the action name is validated as a **single token**
    (bounded length, `[a-z][a-z0-9_]+`):
    no whitespace or shell metacharacters, and no leading `-`,
    so a cloud-provided custom action cannot inject extra arguments or options
    into the init tool or a plugin.
    Multi-word action names are rejected:
    a custom action is a *name* the backend understands, not a command line to execute;
  * the service name is validated
    (non-empty, bounded length, `[A-Za-z0-9_.@-]+`, no leading `-`
    to prevent option injection into the init tool);
  * the service type is validated (`[a-z0-9_-]+`)
    since it selects a file under the plugin directory (path-traversal guard);
  * all execution is argv-based; cloud-provided values are never interpolated into a shell.
* **`/usr/share/tedge/service-plugins/` must be root-owned and not writable by the `tedge` user**
  (packaging creates it `root:root 755`).
  Since `tedge service` runs as root,
  a tedge-writable plugin directory would be a trivial privilege escalation.
  This is the same property the sudoers path restriction enforces for sm-plugins today.

## Future consideration

### Service capability discovery: Mix of push-model and pull-model

For a better user experience, tedge-agent should be able to discover service capabilities
and even register them with thin-edge.

Below is a rough sketch of what we could support:
* Introduce `list` subcommand to service plugin.
  * `tedge-agent` queries the service plugin when the service registers.
(e.g. invoking `/usr/share/tedge/service-plugins/<plugin> list`)
  * If `list` is not implemented, this indicates the service will declare their capabilities on their own.
  * `list` does registration as well.
* For a newly installed service, use `sync` to reload the list.

### Filter the capability to declare to cloud

Today, every `te/device/<device>/service/<service>/cmd/+` topic is treated as a capability
declared to the cloud.
If a user wants to limit which capabilities are declared, how do we support that?

A service declaring both `cmd/restart` and `cmd/collect_measurements`
gets both in `c8y_SupportedServiceCommands`, with no way to expose only one.
