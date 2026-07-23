# Service command support (Cumulocity service actions)

* Date: __2026-07-23__
* Status: __Draft__

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
  services declare which commands they support and receive commands on the standard `te/.../cmd/...` topics.
  The Cumulocity specifics live entirely in the c8y mapper.
* Support the standard commands `start`, `stop`, `restart`, plus **custom commands**:
  Cumulocity allows arbitrary command names in `c8y_SupportedServiceCommands`,
  and a service owner can define its own.
* Works both for init-managed services (abstracted by `system.toml`)
  and for services controlled through a third-party abstraction (e.g. containers via `tedge-container-plugin`),
  without conflicting executors.
* No broadening of the privileged surface beyond what thin-edge packaging already grants.

## Design

At a glance, four roles are involved:

* **Service owner** (tedge-agent, a mapper, or a third-party daemon like `tedge-container-plugin`):
  registers the service as an entity and is the source of its supported commands.
* **tedge-mapper-c8y**: converts between Cumulocity and thin-edge
    * the capability → `c8y_SupportedServiceCommands` + supported operation (SmartREST `114`)
    * a `c8y_ServiceCommand` operation → a thin-edge command;
    * the command status → SmartREST `501`–`506`.
* **tedge-agent**: executes the command with its workflow engine,
  delegating to the new `tedge service` CLI.
* **Service plugin** (per service type, optional):
  executes the commands of services not managed by the init system (e.g. containers).

### thin-edge interface


#### Command shape: per-command model (`cmd/start`, `cmd/stop`, `cmd/restart`, `cmd/<custom>`)

A service command is addressed as below:

* Capability: `te/device/<device>/service/<service>/cmd/<start|stop|restart|custom>`
  ```json
  {}
  ```
* Command: `te/device/<device>/service/<service>/cmd/restart/<cmd-id>`
  ```json
  {
    "status": "init",
    "command": "restart",
    "serviceName": "tedge-mapper-c8y",
    "serviceType": "service"
  }
  ```
* This is arguably the more natural thin-edge API, and allows a distinct workflow per command.
* It needs the workflow engine to scope workflows by entity type,
  so that a service `restart` does not also trigger the device reboot workflow.
  -> Add `type = "service"` filter in the workflow definition.
* (c8y) The c8y mapper has to aggregate an open set of `cmd/<x>` topics into one `c8y_SupportedServiceCommands` list.

##### JSON field
* `status`: the states used by workflow
* `command`: the command parsed from cloud operation (lower-case)
* `serviceName`: the service's name parsed from cloud operation (e.g. `tedge-mapper-c8y`)
* `serviceType`: the service's type. Key to select a right service plugin (e.g. `service`, `container`)

##### (c8y) Aggregation of capabilities

* On mapper restart, the retained `cmd/<x>` messages replay,
  so the mapper rebuilds the full set and no capability is lost.
* A capability is removed when its `cmd/<x>` topic is cleared (retained empty message).
  The mapper drops it from the set and re-publishes the reduced `c8y_SupportedServiceCommands` array.

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
* **Operation → command.**
  The mapper natively understands the `c8y_ServiceCommand` fragment from the JSON-over-MQTT operation channel
  (first-class, *not* a shipped custom operation handler file):
  * the target entity is resolved from `externalSource.externalId`.
    Unresolvable targets fail the cloud operation with a clear reason.
  * `command` is validated case-insensitively against the declared capabilities.
    An undeclared command fails the cloud operation.
    In addition, the command must pass the single-token validation
    described under Security considerations;
  * `serviceName` is derived from the resolved entity topic id, not from the cloud payload
    (the cloud value may be a display name);
  * `serviceType` is taken from the service's registration data (its `type` property),
    falling back to the payload value.
    Declaring the service type at registration is optional today,
    but effectively becomes mandatory for services using this feature:
    a service registered without a type is dispatched as the default type `service` (init-managed).
* **Status → Cumulocity.**
  * The existing command-status mapping (SmartREST `501`–`506`) already supports service entities,
  so nothing new is required.

### Execution: central executor with pluggable dispatch

**Exactly one executor per device**: the tedge-agent workflow engine,
extended to also react to commands addressed to *services of its own device*
(today it only reacts to commands addressed to the device itself).

**Opening up the workflow engine, deliberately narrow.**
This requires a new subscription in tedge-agent, and its scope matters:

* it covers only services of the agent's **own** device (`te/device/<own-device>/service/+/cmd/...`),
  never services of other devices.
* Introduce a `type` field in the workflow definition to avoid name collision between different targets.
  (e.g. `restart` for a device and a service must have a different workflow.)
  ```toml
  operation = "restart"
  type = "service" # Can take one of the `@type` values: <device|child-device|service>
  ```

Note: This topic filter only works with the default topic scheme
(`te/device/<device>/service/<service>`).
The proper way to find "services of my device" is to use the entity store,
as custom topic schemes keep the device–service relation only there.

The model is per-device.
A child device running its own tedge-agent executes the commands for its own services,
using its own `system.toml`, its own `tedge` CLI/sudoers rule and its own service plugins.
Nothing is looked up across devices.

### New CLI command `tedge service` and service plugin

The workflow's execution step delegates to a new CLI command:

```
sudo -n tedge service <command> <service-name> [--service-type <type>]
```

where `<command>` is a validated single token, e.g. `start`, `stop` or `restart`.
Which commands are supported is decided by the backend,
as `tedge service` dispatches on the service type:

| service type          | backend                                                                 |
| --------------------- | ----------------------------------------------------------------------- |
| `service` (default)   | built-in init-system abstraction (`/etc/tedge/system.toml`): a command is supported if `system.toml` defines a command template for it; the standard commands out of the box, and custom commands (e.g. `reload`) by adding an entry to `system.toml` |
| any other type        | **service plugin**: `/usr/share/tedge/service-plugins/<type> <command> <name>`; the command is passed through, the plugin decides what it supports |
| unsupported command / no plugin for the type | `tedge service` fails with a distinct exit code and a clear error message |

Note: the current `system.toml` schema is fixed
(unknown keys are rejected via `deny_unknown_fields`),
so accepting additional custom command templates requires a small schema extension.

#### Service plugin contract

* an executable at `/usr/share/tedge/service-plugins/<service-type>`.
* invoked as `<plugin> <command> <service-name>`,
  where `<command>` is `start`, `stop`, `restart` or a plugin-defined custom command.
* exit `0` on success; non-zero on failure with the reason on stderr;
* a reserved exit code for "command not supported for this type".
* custom command names must be lowercase-safe, because the operation name is used in the topic.
  For example, `myCommand` must be accepted as `mycommand`.

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
  The workflow engine has no per-entity filtering,
  so once the agent subscribes, it drives the state machine for *every* service of its device.
  This design makes that a deliberate contract instead of an accident:
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
  The shipped workflow instead uses the existing self-restart pattern
  (detached background execution followed by an `await-agent-restart` state),
  the same pattern already documented for configuration self-update.
* `stop` of **tedge-agent** (the executor) and of any **cloud mapper** (e.g. tedge-mapper-c8y)
  is rejected with a clear failure reason.
  A stopped executor cannot report completion, and a stopped mapper loses the cloud connection,
  so the operation would hang in the cloud forever.

The workflow carries a timeout so that a hung backend surfaces as a clean failure rather than a stuck operation.

## End-to-end examples

All examples assume the main device with the default topic scheme.

### A systemd service with the standard commands

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
  "command": "restart",
  "serviceName": "collectd",
  "serviceType": "service"
}
```

Then the `restart` workflow of tedge-agent drives the command.

File: `/etc/tedge/operations/restart.toml`
```toml
operation = "restart"
type = "service"

[init]
  action = "proceed"
  on_success = "executing"

[executing]
  action = "proceed"
  on_success = "run"

[run]
  script = "sudo -n tedge service ${.topic.operation} ${.payload.serviceName} --service-type ${.payload.serviceType}"
  on_success = "successful"
  on_error = { status = "failed", reason = "Command returned a non-zero exit code" }

[successful]
  action = "cleanup"

[failed]
  action = "cleanup"
```

* The `run` script resolves to `sudo -n tedge service restart collectd --service-type service`.
* Then type `service` routes to `systemctl restart collectd`.
* Exit code `0` → status `successful` → the mapper reports `506`.

### A container service with standard and custom commands

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

A user creates an operation with the RESTART button in Cumulocity,
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
  "command": "pause",
  "serviceName": "nodered",
  "serviceType": "container"
}
```

Then, the same workflow script can drive the command:

File: `/etc/tedge/operations/pause.toml`
```toml
operation = "pause"
type = "service"

[init]
  action = "proceed"
  on_success = "executing"

[executing]
  action = "proceed"
  on_success = "run"

[run]
  script = "sudo -n tedge service ${.topic.operation} ${.payload.serviceName} --service-type ${.payload.serviceType}"
  on_success = "successful"
  on_error = { status = "failed", reason = "Command returned a non-zero exit code" }

[successful]
  action = "cleanup"

[failed]
  action = "cleanup"
```

* The `run` script resolves to `sudo -n tedge service pause nodered --service-type container`,
which executes `/usr/share/tedge/service-plugins/container pause nodered`.
* The plugin maps the command to its container engine (e.g. `docker pause`).

Example service plugin script:
```sh
#!/bin/sh
# Service plugin for the "container" service type.
# Installed as: /usr/share/tedge/service-plugins/container
# Invoked by `tedge service` as: container <command> <service-name>
#
# Exit codes:
#   0   success
#   1   command failed (reason on stderr)
#   2   command not supported by this plugin (reserved)
set -eu

COMMAND="$1"
NAME="$2"

case "$COMMAND" in
    start)   docker start   "$NAME" ;;
    stop)    docker stop    "$NAME" ;;
    restart) docker restart "$NAME" ;;
    pause)   docker pause   "$NAME" ;;
    unpause) docker unpause "$NAME" ;;
    *)
        echo "container plugin: unsupported command '$COMMAND'" >&2
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
  * the command is validated as a **single token**
    (non-empty, bounded length, `[A-Za-z0-9_-]+`, no leading `-`):
    no whitespace or shell metacharacters,
    so a cloud-provided custom command cannot inject extra arguments or options
    into the init tool or a plugin.
    Multi-word command strings are rejected:
    a custom command is a *name* the backend understands, not a command line to execute;
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

### Workflow file naming collision

Today a workflow is stored in `/etc/tedge/operations/<operation>.toml`.
The new `type` field lets a device `restart` and a service `restart` coexist as separate workflows,
but two files named `restart.toml` cannot live in the same directory.
How the agent names these files on disk is still open.
