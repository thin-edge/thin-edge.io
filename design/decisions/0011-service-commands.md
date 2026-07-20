# Service command support (Cumulocity service actions)

* Date: __2026-07-07__
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
because **services on one device are owned by different daemons**:

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
* Works both for init-managed services and for services owned by third-party daemons,
  without conflicting executors.
* Ownership is respected:
  what a service supports reflects what its owner can actually execute.
* No broadening of the privileged surface beyond what thin-edge packaging already grants.

## Design

At a glance, four roles are involved:

* **Service owner** (tedge-agent, a mapper, or a third-party daemon)
* **tedge-mapper-c8y**: converts between Cumulocity and thin-edge
    * the capability → `c8y_SupportedServiceCommands` + supported operation (SmartREST `114`)
    * a `c8y_ServiceCommand` operation → a thin-edge command;
    * the command status → SmartREST `501`–`506`.
* **tedge-agent**: executes the command with its workflow engine,
  delegating to the new `tedge service` CLI.
* **Service plugin** (per service type, optional):
  executes the commands of services not managed by the init system (e.g. containers).

### thin-edge interface (Open decisions)

#### Command shape: one-command vs per-command vs mixed

How is a service command addressed on the `te/.../cmd/...` topics?
There are three options:

##### Proposal 1: One command `cmd/service_command`
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
* The single command model can limit the commands being executed in parallel.
* (c8y) The c8y mapper copies the declared commands straight into `c8y_SupportedServiceCommands`.

##### Proposal 2: Per command `cmd/start`, `cmd/stop`, `cmd/restart`, `cmd/<custom>`
Capability: `te/device/<device>/service/<service>/cmd/<start|stop|restart|custom>`
```json
{}
```
Command: `te/device/<device>/service/<service>/cmd/restart/<cmd-id>`
```json
{
  "status": "init",
  "serviceName": "tedge-mapper-c8y",
  "serviceType": "service"
}
```
* This is arguably the more natural thin-edge API, and allows a distinct workflow per command.
* It needs the workflow engine to scope workflows by entity type,
  so that a service `restart` does not also trigger the device reboot workflow.
* (c8y) The c8y mapper has to aggregate an open set of `cmd/<x>` topics into one `c8y_SupportedServiceCommands` list.

##### Proposal 3: Mix of above
* Use the basic set (`start`, `stop`, `restart`, `enable`, `disable`) under `cmd/service_command`.
* Custom commands have their own `cmd/<custom>`.
* (c8y) This still needs the aggregation logic same as per-command model.

##### Proposal 1-3 Common open questions
`serviceName` is still to be decided:
* service's name? e.g. `nodered`
* service's external ID? e.g. `tedge_7fe9b92c:device:main:service:nodered`
* topic ID? e.g. `device/main/service/nodered`
* or do we even need it at all?

#### Capability declaration: Push model vs Pull model

A service's supported commands are published as a retained capability message on its `cmd` topic.
Who publishes it?

##### Proposal A: Push-model
The service owner publishes it itself.

* `tedge-agent` and the mappers declare it for their own service entity at startup,
  alongside their existing service registration / health announcement.
* A third-party daemon declares it for each service it owns (it already registers those entities).

##### Proposal B: Pull-model
`tedge-agent` publishes it on the owner's behalf.
* For init-managed services: it derives the commands from what `system.toml` defines.
  * Problem: How to limit supported commands for some services?
  For example, cloud mappers should not allow `stop`,
  otherwise the cloud connection would be lost.
* For other types: it queries the service plugin when the service registers.
(e.g. invoking `/user/share/tedge/service-plugins/<plugin> list`)

### Cumulocity mapping (will be updated depending on the command shape decision)

This part deliberately reuses existing mapper mechanisms; no new concepts are introduced.

* **Capability → Cumulocity.**
  When the mapper receives a `service_command` capability from a service entity, it:
  1. registers `c8y_ServiceCommand` as a supported operation for that service,
     reusing the same mechanism that today registers `c8y_Restart`
     when a service declares a restart capability
     (per-entity operation registration + SmartREST `114`).
     This mechanism explicitly reloads the service's supported-operation set,
     so the inotify limitation for service operation directories (see Background) does not apply;
  2. sets the declared types as `{"c8y_SupportedServiceCommands": ["START", "STOP", ...]}`
     (uppercased, following the Cumulocity convention) on the service's managed object,
     either via the `twin` topic of the service entity
     or by publishing an inventory update through the JSON over MQTT API;
     both existing mechanisms already address service managed objects.
     Since Cumulocity accepts arbitrary command names,
     custom types are passed through unchanged apart from the case mapping.
* **Operation → command.**
  The mapper natively understands the `c8y_ServiceCommand` fragment from the JSON-over-MQTT operation channel
  (first-class, like `c8y_Restart`, *not* a shipped custom operation handler file):
  * the target entity is resolved from `externalSource.externalId`;
    unresolvable targets fail the cloud operation with a clear reason;
  * `command` is validated case-insensitively against the **types the service declared** in its capability
    (standard and custom commands are handled in the same way);
    an undeclared command fails the cloud operation.
    In addition, the command must pass the single-token validation
    described under Security considerations;
  * `serviceName` is derived from the resolved entity topic id, not from the cloud payload
    (the cloud value may be a display name);
    `serviceType` is taken from the service's registration data (its `type` property),
    falling back to the payload value.
    Declaring the service type at registration is optional today,
    but effectively becomes mandatory for services using this feature:
    a service registered without a type is dispatched as the default type `service` (init-managed).
* **Status → Cumulocity.**
  The existing command-status mapping (SmartREST `501`–`506`) already supports service entities
  (status is published against the service's external id);
  nothing new is required.
* The feature is gated by a `c8y.enable.service_command` setting,
  following the existing `c8y.enable.*` pattern (default: enabled).

### Execution: central executor with pluggable dispatch (will be updated depending on the command shape decision)

**Exactly one executor per device**: the tedge-agent workflow engine,
extended to also react to commands addressed to *services of its own device*
(today it only reacts to commands addressed to the device itself).
A `service_command` workflow is shipped with tedge-agent
(user-overridable, like other built-in workflow definitions).
It does **not** self-declare a capability on the main device,
because capability declaration belongs to service owners (see above).

**Opening up the workflow engine, deliberately narrow.**
This requires a new subscription in tedge-agent, and its scope matters:

* it covers only services of the agent's **own** device (`te/device/<own-device>/service/+/cmd/...`),
  never services of other devices;
* it covers only the **`service_command` channel** (`.../cmd/service_command/+`), not all commands:
  with a subscription to all commands,
  every *other* custom workflow loaded on the agent would suddenly also fire for service-addressed commands.
  Narrowing to the one operation confines the behavior change to this feature.

This topic filter only works with the default topic scheme
(`te/device/<device>/service/<service>`).
The proper way to find "services of my device" is to use the entity store,
as custom topic schemes keep the device–service relation only there.
Still, the first iteration uses the topic filter as it is simple;
custom topic schemes are not supported yet.

The model is per-device, not centralized:
a child device running its own tedge-agent executes the commands for its own services,
using its own `system.toml`, its own `tedge` CLI/sudoers rule and its own service plugins.
Nothing is looked up across devices.

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
| any other type        | **service plugin**: `/usr/share/tedge/service-plugins/<type> <command> <name>`; the command (standard or custom) is passed through, the plugin decides what it supports |
| unsupported command / no plugin for the type | `tedge service` fails with a distinct exit code and a clear error message |

Note: the current `system.toml` schema is fixed
(unknown keys are rejected via `deny_unknown_fields`),
so accepting additional custom command templates requires a small schema extension.

**Service plugin contract** (new, intentionally minimal, mirroring the sm-plugins pattern):

* an executable at `/usr/share/tedge/service-plugins/<service-type>`;
* invoked as `<plugin> <command> <service-name>`,
  where `<command>` is `start`, `stop`, `restart` or a plugin-defined custom command;
* exit `0` on success; non-zero on failure with the reason on stderr;
* a reserved exit code for "command not supported for this type".

**Example: tedge-container-plugin.**
The plugin registers container services with types `container` and `container-group`
and already exposes container lifecycle primitives through its CLI.
Integration is two symlinks (`/usr/share/tedge/service-plugins/container`, `.../container-group`)
onto a thin subcommand mapping `start|stop|restart <name>` to its existing container operations:
no MQTT protocol to implement, no state machine, no operation handling code.
It is also free to support custom commands of its own (e.g. `pause`, `unpause`),
declared in the capability of the services it registers.

Why this shape:

* **The command protocol allows only one state-machine driver per command topic anyway.**
  A command topic is a shared, retained state record:
  if two components reacted to the same command
  (e.g. the agent's workflow engine *and* an owner daemon),
  they would publish conflicting state transitions on the same topic and corrupt each other,
  independently of who is "right".
  The workflow engine has no per-entity filtering,
  so once the agent subscribes, it drives the state machine for *every* service of its device.
  This design makes that a deliberate contract instead of an accident:
  **tedge-agent is the sole driver of `service_command` for its device's services,
  and third parties integrate *below* the state machine (an executed plugin process),
  never *beside* it (a competing MQTT subscriber)**.
  Consequently, in this iteration,
  declaring the `service_command` capability implies execution through this central dispatch.
* **Single executor ⇒ no terminal-state hazard.**
  Since nothing else will ever execute the command,
  failing with "no handler installed for service type `<type>`" is *correct*,
  not a race against the real owner (constraint 1).
* The `tedge service` command is independently useful for operators:
  an init-system-agnostic wrapper (`tedge service restart mosquitto`) that honors `system.toml`.
  Today this requires knowing which init system the device uses.
* It resolves two practical execution problems at once:
  workflow scripts cannot read `system.toml`, and service actions need root.
  `tedge` under `sudo -n` is already authorized by the shipped sudoers configuration (constraint 5),
  so no packaging change is needed.

**Self-targeting services** (the executor acting on itself or its peers):

* `restart` of **tedge-agent** itself must not run as a plain synchronous step:
  the agent is killed mid-workflow and, on resume, the step would re-execute: a restart loop.
  The shipped workflow instead uses the existing self-restart pattern
  (detached background execution followed by an `await-agent-restart` state),
  the same pattern already documented for configuration self-update.
* `stop` of **tedge-agent** is rejected with a clear failure reason:
  a stopped agent can never report completion, so the operation would hang in the cloud forever.
* `restart` of **tedge-mapper-c8y** needs no special handling:
  command states are retained on the local broker,
  and the mapper processes retained command updates on reconnect,
  so the final success status reaches the cloud after the mapper is back.

The workflow carries a timeout so that a hung backend surfaces as a clean failure rather than a stuck operation.

### End-to-end examples

All examples assume the main device with the default topic scheme.

#### A systemd service with the standard commands

`tedge-mapper-c8y` declares its own capability at startup (retained):

```
[te/device/main/service/tedge-mapper-c8y/cmd/service_command]
{ "types": ["start", "stop", "restart", "enable", "disable"] }
```

The c8y mapper reacts to this capability message:

* registers `c8y_ServiceCommand` as a supported operation of the service (SmartREST `114`);
* sets `{"c8y_SupportedServiceCommands": ["START", "STOP", "RESTART", "ENABLE", "DISABLE"]}`
  on the service's managed object.

A user presses the RESTART button,
and the mapper receives the `c8y_ServiceCommand` operation:

```
{ "c8y_ServiceCommand": { "serviceType": "service", "serviceName": "tedge-mapper-c8y", "command": "RESTART" } }
```

It resolves the target entity from the external id,
validates `restart` against the declared types,
and publishes the command:

```
[te/device/main/service/tedge-mapper-c8y/cmd/service_command/c8y-mapper-123]
{ "status": "init", "command": "restart", "serviceName": "tedge-mapper-c8y", "serviceType": "service" }
```

The `service_command` workflow of tedge-agent drives the command:
status `executing` (the mapper reports `504`), then its script step runs:

```
script = "sudo -n tedge service ${.payload.command} ${.payload.serviceName} --service-type ${.payload.serviceType}"
```

which resolves to `sudo -n tedge service restart tedge-mapper-c8y --service-type service`.
The type `service` routes to `system.toml`: `systemctl restart tedge-mapper-c8y`.
Exit code `0` → status `successful` → the mapper reports `506`.
(Since this restarts the mapper itself, the final status is reported after the mapper reconnects.)

#### A container service with standard and custom commands

tedge-container-plugin registers `nodered` with type `container` and declares:

```
[te/device/main/service/nodered/cmd/service_command]
{ "types": ["start", "stop", "restart", "pause", "unpause"] }
```

A PAUSE operation becomes:

```
[te/device/main/service/nodered/cmd/service_command/c8y-mapper-124]
{ "status": "init", "command": "pause", "serviceName": "nodered", "serviceType": "container" }
```

The same workflow script resolves to
`sudo -n tedge service pause nodered --service-type container`,
which executes `/usr/share/tedge/service-plugins/container pause nodered`.
The plugin maps the command to its container engine (e.g. `docker pause`).
A command the plugin does not support ends with the reserved exit code,
and the workflow fails the command with a clear reason.

## Alternative considered: distributed executors

Each owner executes the commands for its own services:
tedge-agent handles only init-managed services;
tedge-container-plugin (and any other owner daemon) subscribes to `.../cmd/service_command/+` for its services
and drives the `init → executing → successful|failed` state machine itself.

This is the philosophically cleanest model
(the declarer of a capability is also its executor)
and the most flexible long-term (owners control execution semantics: async operations, progress reporting).
It was not chosen for the first iteration because:

1. **It requires "silently ignore" machinery in the agent.**
   Because cloud terminal states are final (constraint 1),
   the agent must publish *nothing at all* for commands targeting services it does not own.
   Even an honest "failed: not my service" would permanently destroy the operation for the real owner.
   The workflow engine cannot do this today:
   it would need per-workflow target scoping informed by entity registration data (constraint 2),
   a new engine feature with its own design questions.
2. **It puts a protocol burden on every third party.**
   Each owner daemon must implement the thin-edge command protocol.
   For tedge-container-plugin this is entirely new code (constraint 3),
   compared with two symlinks in the proposed model.
3. **It has a bad failure mode.**
   If the owner daemon is absent, wrongly deployed or crashed,
   its operations sit PENDING/EXECUTING in the cloud forever,
   as no other component is entitled to fail them.

The proposed model does not block moving to distributed executors later:
if an owner needs execution semantics the central dispatch cannot provide,
the follow-up work is type-scoped command dispatch in the agent (informed by the entity store),
at which point that service type is removed from central dispatch.
The owner-declares-capability rule is chosen now precisely so that this evolution requires no interface change.

### Alternative for the executor: a built-in operation instead of a shipped workflow

`service_command` could be implemented as a **built-in operation** of tedge-agent,
like software management,
where a Rust actor spawns the external executables (sm-plugins) directly as child processes.
Note that this is a smaller difference than it may appear:
built-in operations are still supervised by the same workflow engine and need the same new subscription;
the choice is only *what executes the action*:
a shipped workflow definition with a script step, or agent code.

This proposal prefers the shipped workflow for the first iteration:

* **The security-critical validation sits in `tedge service` either way.**
  The CLI is required regardless, as the privileged helper
  (the agent does not run as root; `sudo -n tedge` is already authorized).
  A built-in operation would not remove that single validation point;
  it would only duplicate the validation in a second place.
* **Transparency and customizability.**
  With a workflow definition,
  the exact behavior is visible and adjustable on the device without recompiling,
  e.g. routing a particular custom command differently, or tightening the self-targeting rules.
  This matches the open-ended custom-command set.
* **The hard case is already solved at the workflow level.**
  Self-restart of tedge-agent uses an existing, documented workflow pattern
  (detached background execution + `await-agent-restart`);
  a built-in operation would have to re-implement equivalent restart-persistence logic in code.
* **Substantially less new agent code.**
  The agent-side logic is a thin dispatch that the CLI already implements.

What a built-in operation would buy
(validating the service type against the agent's own entity store rather than the mapper-provided payload value,
and code-level self-restart handling)
does not seem to justify a new actor,
since the mapper already derives `serviceType` from its entity cache
(registration truth, not the raw cloud payload) before creating the command.
If this judgement changes,
a built-in operation can later replace the shipped workflow
without changing any external interface (topics, CLI, plugin contract),
and would remain user-overridable via the existing `builtin:` workflow-customization mechanism.

### Alternative for the mapper part: custom operation handler files

Instead of native mapper support,
the conversion can be assembled from existing generic building blocks:
a custom-operation handler file (`on_fragment = "c8y_ServiceCommand"`) plus a user-provided workflow.
This works, but was rejected for the product feature:
it requires per-service operation file management by the user,
inherits the limited dynamic loading of service operation directories,
leaves capability declaration (`c8y_SupportedServiceCommands`, SmartREST `114`) as manual steps,
and ties the feature to Cumulocity-specific configuration files
instead of the cloud-agnostic capability/command interface.

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

## Open questions

1. Who declares the capability for init-managed services that are not owned by a thin-edge daemon (e.g. `mosquitto`)?
   With owner-declared capabilities,
   such services get no cloud action buttons even though the executor could handle them.
   A configuration-driven list is the likely shape.
