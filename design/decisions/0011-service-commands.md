# Service command support (Cumulocity service actions)

* Date: __2026-07-07__
* Status: __Draft__

## Background

Cumulocity provides an action interface for services. To use it, a service must fulfil two
conditions on the Cumulocity side:

1. `c8y_ServiceCommand` is listed in the service's `c8y_SupportedOperations` fragment.
2. the service's managed object contains the `c8y_SupportedServiceCommands` fragment
   (e.g. `["START", "STOP", "RESTART"]`) listing the commands it supports.

The UI then shows action buttons on that service. Pressing one creates an operation carrying the
`c8y_ServiceCommand` fragment, addressed to the service's external id:

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

thin-edge.io already models services as first-class entities
(`te/device/<device>/service/<service>`) with registration, twin data, telemetry and health
monitoring — but acting on a service is not really supported today:

* **Capability declaration**: a user *can* declare `c8y_ServiceCommand` as a supported operation
  for a service by manually adding an operation file under
  `/etc/tedge/operations/c8y/<service-external-id>/`. However, unlike for the main device, changes
  in child-device and service operation directories are not tracked by inotify, so the file is not
  picked up dynamically — and the `c8y_SupportedServiceCommands` fragment still has to be set on
  the service's managed object by hand.
* **Execution**: tedge-agent's workflow engine currently does not react to *any* commands
  addressed to service entities, so there is no component that would carry out an operation
  targeting a service.

Supporting this is harder than a plain mapper feature because **services on one device are owned
by different daemons**:

1. **Init-managed services** (systemd, OpenRC, SysVinit, ...): thin-edge's own daemons
   (`tedge-agent`, `tedge-mapper-*`) and arbitrary units. thin-edge already abstracts the init
   system behind `/etc/tedge/system.toml`.
2. **Services managed by a third-party daemon**: e.g. containers registered as services by
   [tedge-container-plugin](https://github.com/thin-edge/tedge-container-plugin) with service types
   `container` / `container-group`. Only that daemon knows how to start/stop/restart them.

Converting messages between the Cumulocity format and the thin-edge format is largely a matter of
reusing existing mapper mechanisms; the open design question — and the core of this proposal — is
**who executes a service command, and how**.

## Goals

* A **cloud-agnostic** thin-edge interface: services declare a `service_command` capability and
  receive commands on the standard `te/.../cmd/...` topics; the Cumulocity specifics live entirely
  in the c8y mapper.
* The standard commands `start`, `stop`, `restart`, plus **custom commands**: Cumulocity's
  specification allows arbitrary command names in `c8y_SupportedServiceCommands`, and a service
  owner can declare and handle its own.
* Works both for init-managed services and for services owned by third-party daemons, **without
  conflicting executors**.
* Ownership is respected: only the owner of a service declares what that service supports.
* No broadening of the privileged surface beyond what thin-edge packaging already grants.

## Constraints that shape the design

1. **Cloud terminal states are final.** Once a Cumulocity operation is set to FAILED or
   SUCCESSFUL, its status can never change again. Therefore no component may report a
   failed/successful status for a command that another component might legitimately execute.
   "Fail it with reason: not my service" is only safe when there is provably no other executor.
2. **The tedge-agent workflow engine has no per-entity scoping.** A registered workflow reacts to
   the matching operation on every topic the agent subscribes to; it cannot be restricted to
   services of a particular type, and workflow scripts have no access to entity registration data
   or to `system.toml`.
3. **There is no existing third-party execution model to be compatible with.**
   tedge-container-plugin today has no operation handling at all: it does not subscribe to command
   topics, ships no workflow definitions and no Cumulocity operation handlers. Restarting a
   container from the cloud currently requires the generic shell-command operation. Whatever
   convention this proposal defines *becomes* the third-party integration model.
4. **A delegation precedent already exists**: software management plugins
   (`/etc/tedge/sm-plugins/<type>`) — external executables implementing a fixed CLI contract,
   invoked by tedge-agent. tedge-container-plugin integrates with software management exactly this
   way, and it already has ready-to-use container start/stop/restart primitives in its CLI.
5. **Privilege**: thin-edge packaging already grants the `tedge` user passwordless sudo for
   `/usr/bin/tedge` (among a small set of paths). `systemctl` is *not* whitelisted, and the
   `system.toml` service-manager abstraction is only usable from the `tedge` CLI today.

## Design

### Thin-edge interface (cloud-agnostic)

**Capability** — declared by the *service owner* as a retained message:

```
[te/device/main/service/tedge-mapper-c8y/cmd/service_command]
{ "types": ["start", "stop", "restart"] }
```

* `tedge-agent` and the mappers declare it for their own service entity at startup, alongside
  their existing service registration / health announcement.
* A third-party daemon declares it for each service it owns (it already registers those entities).
* The payload key `types` follows the existing capability conventions (config types, log types).
* `types` is an open set: besides the standard `start`, `stop`, `restart`, an owner may declare
  arbitrary custom command names (e.g. `flush-cache`) that its services support. A command name is
  a single token (no whitespace; see Security considerations).

**Command** — the standard thin-edge command state machine on the target *service* topic:

```
[te/device/main/service/tedge-mapper-c8y/cmd/service_command/<cmd-id>]
{
  "status": "init",
  "command": "restart",
  "serviceName": "tedge-mapper-c8y",
  "serviceType": "service"
}
```

with the usual `init → executing → successful | failed` transitions published on the same topic.

### Cumulocity mapping

This part deliberately reuses existing mapper mechanisms; no new concepts are introduced.

* **Capability → Cumulocity.** When the mapper receives a `service_command` capability from a
  service entity, it:
  1. registers `c8y_ServiceCommand` as a supported operation for that service — reusing the same
     mechanism that today registers `c8y_Restart` when a service declares a restart capability
     (per-entity operation registration + SmartREST `114`). This mechanism explicitly reloads the
     service's supported-operation set, so the inotify limitation for service operation
     directories (see Background) does not apply;
  2. sets the declared types as `{"c8y_SupportedServiceCommands": ["START", "STOP", ...]}`
     (uppercased, following the Cumulocity convention) on the service's managed object — either
     via the `twin` topic of the service entity or by publishing an inventory update through the
     JSON over MQTT API; both existing mechanisms already address service managed objects. Since
     Cumulocity accepts arbitrary command names, custom types are passed through unchanged apart
     from the case mapping.
* **Operation → command.** The mapper natively understands the `c8y_ServiceCommand` fragment from
  the JSON-over-MQTT operation channel (first-class, like `c8y_Restart` — *not* a shipped custom
  operation handler file):
  * the target entity is resolved from `externalSource.externalId`; unresolvable targets fail the
    cloud operation with a clear reason;
  * `command` is validated case-insensitively against the **types the service declared** in its
    capability (standard and custom commands are handled uniformly); an undeclared command fails
    the cloud operation. In addition, the command must pass the syntactic single-token validation
    described under Security considerations;
  * `serviceName` is derived from the resolved entity topic id, not from the cloud payload (the
    cloud value may be a display name); `serviceType` is taken from the entity's registered twin
    data, falling back to the payload value.
* **Status → Cumulocity.** The existing command-status mapping (SmartREST `501`–`506`) already
  supports service entities (status is published against the service's external id); nothing new
  is required.
* The feature is gated by a `c8y.enable.service_command` setting, following the existing
  `c8y.enable.*` pattern (default: enabled).

### Execution: central executor with pluggable dispatch

**Exactly one executor per device**: the tedge-agent workflow engine, extended to also react to
commands addressed to *services of its own device* (today it only reacts to commands addressed to
the device itself). A `service_command` workflow is shipped with tedge-agent (user-overridable,
like other built-in workflow definitions). It does **not** self-declare a capability on the main
device — capability declaration belongs to service owners (see above).

**Opening up the workflow engine — deliberately narrow.** This requires a new subscription in
tedge-agent, and its scope matters:

* it covers only services of the agent's **own** device
  (`te/device/<own-device>/service/+/cmd/...`), never services of other devices;
* it covers only the **`service_command` channel**
  (`.../cmd/service_command/+`), not all commands: with a blanket subscription, every *other*
  custom workflow loaded on the agent would suddenly also fire for service-addressed commands —
  narrowing to the one operation confines the behavior change to this feature.

The model is per-device, not centralized: a child device running its own tedge-agent executes the
commands for its own services, using its own `system.toml`, its own `tedge` CLI/sudoers rule and
its own service plugins — nothing is looked up across devices.

The workflow's execution step delegates to a new CLI command:

```
sudo -n tedge service <command> <service-name> [--service-type <type>]
```

where `<command>` is a validated single token — `start`, `stop`, `restart` or a custom command
name. `tedge service` dispatches on the service type:

| service type          | backend                                                                 |
| --------------------- | ----------------------------------------------------------------------- |
| `service` (default)   | built-in init-system abstraction (`/etc/tedge/system.toml`): a command is supported if `system.toml` defines a command template for it — `start`, `stop`, `restart` out of the box, and custom commands (e.g. `reload`) by adding an entry to `system.toml` |
| any other type        | **service plugin**: `/etc/tedge/service-plugins/<type> <command> <name>`; the command (standard or custom) is passed through, the plugin decides what it supports |
| unsupported command / no plugin for the type | `tedge service` fails with a distinct exit code and a clear error message |

**Service plugin contract** (new, intentionally minimal, mirroring the sm-plugins pattern):

* an executable at `/etc/tedge/service-plugins/<service-type>`;
* invoked as `<plugin> <command> <service-name>`, where `<command>` is `start`, `stop`, `restart`
  or a plugin-defined custom command;
* exit `0` on success; non-zero on failure with the reason on stderr;
* a reserved exit code for "command not supported for this type".

**Example: tedge-container-plugin.** The plugin registers container services with types
`container` and `container-group` and already exposes container lifecycle primitives through its
CLI. Integration is two symlinks (`/etc/tedge/service-plugins/container`, `.../container-group`)
onto a thin subcommand mapping `start|stop|restart <name>` to its existing container operations —
no MQTT protocol to implement, no state machine, no operation handling code. It is also free to
support custom commands of its own (e.g. `pause`, `unpause`), declared in the capability of the
services it registers.

Why this shape:

* **The command protocol admits only one state-machine driver per command topic anyway.** A
  command topic is a shared, retained state record: if two components reacted to the same command
  (e.g. the agent's workflow engine *and* an owner daemon), they would publish conflicting state
  transitions on the same topic and corrupt each other — independently of who is "right". The
  workflow engine has no per-entity filtering, so once the agent subscribes, it drives the state
  machine for *every* service of its device. This design makes that a deliberate contract instead
  of an accident: **tedge-agent is the sole driver of `service_command` for its device's
  services, and third parties integrate *below* the state machine (an executed plugin process),
  never *beside* it (a competing MQTT subscriber)**. Consequently, in this iteration, declaring
  the `service_command` capability implies execution through this central dispatch.
* **Single executor ⇒ no terminal-state hazard.** Since nothing else will ever execute the
  command, failing with "no handler installed for service type `<type>`" is *correct*, not a
  race against the real owner (constraint 1).
* The `tedge service` command is independently useful for operators: an init-system-agnostic
  wrapper (`tedge service restart mosquitto`) that honors `system.toml` — something that today
  requires knowing which init system the device uses.
* It resolves two practical execution problems at once: workflow scripts cannot read
  `system.toml`, and service actions need root — `tedge` under `sudo -n` is already authorized by
  the shipped sudoers configuration (constraint 5), so no packaging change is needed.

**Self-targeting services** (the executor acting on itself or its peers):

* `restart` of **tedge-agent** itself must not run as a plain synchronous step: the agent is
  killed mid-workflow and, on resume, the step would re-execute — a restart loop. The shipped
  workflow instead uses the existing self-restart idiom (detached background execution followed by
  an `await-agent-restart` state), the same pattern already documented for configuration
  self-update.
* `stop` of **tedge-agent** is rejected with a clear failure reason: a stopped agent can never
  report completion, so the operation would hang in the cloud forever.
* `restart` of **tedge-mapper-c8y** needs no special handling: command states are retained on the
  local broker, and the mapper processes retained command updates on reconnect, so the final
  success status reaches the cloud after the mapper is back.

The workflow carries a timeout so that a hung backend surfaces as a clean failure rather than a
stuck operation.

## Alternative considered: distributed executors

Each owner executes the commands for its own services: tedge-agent handles only init-managed
services; tedge-container-plugin (and any other owner daemon) subscribes to
`.../cmd/service_command/+` for its services and drives the `init → executing → successful|failed`
state machine itself.

This is the philosophically cleanest model — the declarer of a capability is also its executor —
and the most flexible long-term (owners control execution semantics: async operations, progress
reporting). It was not chosen for the first iteration because:

1. **It requires "silently ignore" machinery in the agent.** Because cloud terminal states are
   final (constraint 1), the agent must publish *nothing at all* for commands targeting services
   it does not own — even an honest "failed: not my service" would permanently destroy the
   operation for the real owner. The workflow engine cannot do this today: it would need
   per-workflow target scoping informed by entity registration data (constraint 2), a new engine
   feature with its own design questions.
2. **It puts a protocol burden on every third party.** Each owner daemon must implement the
   thin-edge command protocol. For tedge-container-plugin this is entirely new code (constraint 3),
   compared with two symlinks in the proposed model.
3. **It has a bad failure mode.** If the owner daemon is absent, mis-deployed or crashed, its
   operations sit PENDING/EXECUTING in the cloud forever — no other component is entitled to fail
   them.

The proposed model does not preclude moving to distributed executors later: if an owner needs
execution semantics the central dispatch cannot provide, the follow-up work is type-scoped command
dispatch in the agent (informed by the entity store), at which point that service type is removed
from central dispatch. The owner-declares-capability rule is chosen now precisely so that this
evolution requires no interface change.

### Alternative for the executor: a built-in operation instead of a shipped workflow

`service_command` could be implemented as a **built-in operation** of tedge-agent — like software
management, where a Rust actor spawns the external executables (sm-plugins) directly as child
processes. Note that this is a smaller difference than it may appear: built-in operations are
still supervised by the same workflow engine and need the same new subscription; the choice is
only *what executes the action* — a shipped workflow definition with a script step, or agent code.

This proposal prefers the shipped workflow for the first iteration:

* **The security-critical validation sits in `tedge service` either way.** The CLI is required
  regardless, as the privileged helper (the agent does not run as root; `sudo -n tedge` is already
  authorized). A built-in operation would not remove that choke point — it would only duplicate
  the validation in a second place.
* **Transparency and customizability.** With a workflow definition, the exact behavior is visible
  and adjustable on the device without recompiling — e.g. routing a particular custom command
  differently, or tightening the self-targeting rules. This matches the open-ended custom-command
  set.
* **The hard case is already solved at the workflow level.** Self-restart of tedge-agent uses an
  existing, documented workflow idiom (detached background execution + `await-agent-restart`);
  a built-in operation would have to re-implement equivalent restart-persistence logic in code.
* **Substantially less new agent code.** The agent-side logic is a thin dispatch that the CLI
  already implements.

What a built-in operation would buy — validating the service type against the agent's own entity
store rather than the mapper-provided payload value, and code-level self-restart handling — does
not seem to justify a new actor, since the mapper already derives `serviceType` from its entity
cache (registration truth, not the raw cloud payload) before creating the command. If this
judgement changes, a built-in operation can later replace the shipped workflow without changing
any external interface (topics, CLI, plugin contract), and would remain user-overridable via the
existing `builtin:` workflow-customization mechanism.

### Alternative for the mapper part: custom operation handler files

Instead of native mapper support, the conversion can be assembled from existing generic building
blocks: a custom-operation handler file (`on_fragment = "c8y_ServiceCommand"`) plus a
user-provided workflow. This works, but was rejected for the product feature: it requires
per-service operation file management by the user, inherits the limited dynamic loading of
service operation directories, leaves capability declaration (`c8y_SupportedServiceCommands`,
SmartREST `114`) as manual steps, and ties the feature to Cumulocity-specific configuration files
instead of the cloud-agnostic capability/command interface.

## Security considerations

* **The privileged surface is `tedge service`.** It runs as root via the already-shipped sudoers
  rule for `/usr/bin/tedge`. Consequently:
  * the command is validated as a **single token** (non-empty, bounded length, `[A-Za-z0-9_-]+`,
    no leading `-`): no whitespace or shell metacharacters, so a cloud-provided custom command
    cannot smuggle extra arguments or options into the init tool or a plugin. Multi-word command
    strings are rejected — a custom command is a *name* the backend understands, not a command
    line to execute;
  * the service name is validated (non-empty, bounded length, `[A-Za-z0-9_.@-]+`, no leading `-`
    to prevent option injection into the init tool);
  * the service type is validated (`[a-z0-9_-]+`) since it selects a file under the plugin
    directory (path-traversal guard);
  * all execution is argv-based; cloud-provided values are never interpolated into a shell.
* **`/etc/tedge/service-plugins/` must be root-owned and not writable by the `tedge` user**
  (packaging creates it `root:root 755`). Since `tedge service` runs as root, a tedge-writable
  plugin directory would be a trivial privilege escalation — the same property the sudoers path
  restriction enforces for sm-plugins today.

## Open questions

1. Should `tedge service` also expose `enable|disable|status` (the init abstraction supports
   them)? Proposal: not in v1 — keep the sudo-exposed surface minimal.
2. Who declares the capability for init-managed services that are not owned by a thin-edge daemon
   (e.g. `mosquitto`)? With owner-declared capabilities, such services get no cloud action buttons
   even though the executor could handle them. A configuration-driven list is the likely shape.
