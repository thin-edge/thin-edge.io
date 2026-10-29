## Context

thin-edge cannot act on a service today:
nothing declares which actions a service supports, and nothing executes such a command.
The [proposal](./proposal.md) states the problem and the shape of the feature;
the domain background, the alternatives, and the reasons behind that shape
are in `design/decisions/0011-service-commands.md`.
This document repeats neither.
It records the engineering decisions needed to build the feature in the current code base.

The current state constraining them:

- **Workflow definitions are keyed by operation name only.**
  Nothing in the engine knows which entity type a workflow is for.
- **The agent only subscribes to its own device's commands**,
  and discards the entity part of the topic a command arrives on,
  because there is only one possible target.
- **The init system is abstracted, but with a fixed set of commands**,
  each an argv template with a `{}` placeholder, and unknown keys are rejected.
- **`/usr/bin/tedge` is already a sudoers entry**,
  so a new `tedge` subcommand needs no packaging change to run as root.

## Goals / Non-Goals

**Goals:**

- One executor per device: `tedge-agent` drives every service command,
  and a third party supplies the plugin it runs rather than driving the command itself.
- A workflow can be selected by target entity type, so `log_upload` for a device
  and `log_upload` for a service are separate workflows.
- The c8y mapper handles `c8y_ServiceCommand` natively,
  creating the supported operation entry itself, with nothing to install beforehand.
- `tedge service` is usable on its own, as an init-system-agnostic service wrapper.

**Non-Goals:**

- Discovering service capabilities from the agent (the pull model in 0011's future work).
- Filtering which declared capabilities are exposed to the cloud.
- Reworking how workflow files are named on disk beyond what this feature needs.

## Decisions

### Scope the workflow registry by entity type, not by encoding the type in the operation name

`TomlOperationWorkflow` gains an optional `type` field holding an `EntityType`,
defaulting to `device`.
It must be declared **before** the flattened `handlers` and `states` fields,
otherwise serde folds it into the state map.

The registry key changes from `OperationType` to `(EntityType, OperationType)`
in `WorkflowSupervisor.workflows`.

Alternative considered: encoding the type into the operation name, for example `service/restart`.
Rejected — the operation name appears in the MQTT topic and in the cloud mapping,
so overloading it would leak the scoping into the wire format.

`WorkflowRepository.definitions` (`persist.rs`) is keyed the same way,
and gets the same treatment.

Two methods do not take the entity type.

`get_action` keeps its signature.
A command under execution is identified by its operation name and its `@version`,
and the version is the digest of the workflow definition (`persist.rs`),
of which `type` is part.
So two workflows sharing an operation name never share a version,
and the workflow of a running command is found without knowing the entity type.
This also means a command resumed after an agent restart needs nothing more than
what is already persisted with it, which matters for the agent self-restart case.

`deregistration_message` is only sent for a device workflow.
The capabilities of a service are declared by that service,
so a deleted service workflow file clears nothing on the device's topics.
For the same reason `capability_messages` is filtered on the entity type of the target.

The operation name comes from the `operation` field of the definition, not from the file name,
so a device `log_upload` and a service `log_upload` live in `/etc/tedge/operations/` under any two names.
The only thing a user has to do is set `type = "service"` in the definition.

Alternative considered: a naming convention such as `service-restart.toml`,
or a per-type subdirectory `operations/service/restart.toml`.
Both are unnecessary once the keys carry the entity type,
and the subdirectory would additionally require changing `is_user_defined` (`persist.rs`),
which expects the file's parent to be exactly the workflows directory,
along with the inotify watch that only covers that directory.

### Subscribe to every entity's commands, and ask the entity store who the target is

A topic filter cannot express "services of my device" under a custom topic scheme,
because the device-service relation is carried by the registration message's `@parent`,
not by the topic structure.
So the scoping is not done by the subscription; it is done when a command arrives.

The subscription in `WorkflowActorBuilder::subscriptions` (`builder.rs`) is widened to
`EntityFilter::AnyEntity` with `ChannelFilter::AnyCommand`, keeping the own-service signal filter.
`WorkflowActor::process_mqtt_message` currently drops the entity from the parsed topic (`actor.rs`);
it will keep it and decide, per command, whether to act and under which entity type:

- the target is the device the agent runs on: act, classified as `device`;
- the target is an entity of type `service` whose parent is that device: act, classified as `service`;
- anything else: ignore, with a log line.

The device the agent runs on is recognized by comparing topic identifiers, and needs no lookup.
It is classified as `device` whatever type the entity store holds for it.
An agent on a child device runs its own workflows and reports itself as `child-device`
in the main device's entity store, so classifying it by its reported type would leave it
unable to find its own workflows, and would need a fallback from `(child-device, operation)`
to `(device, operation)`.
Not looking it up also keeps a device command working exactly as before this change:
it does not depend on the entity store being reachable, nor on the device being registered there.
That last point matters under a custom topic scheme, where the device is not registered at all
unless someone registers it: `EntityStore::with_main_device` only ever holds `device/main//`
(`agent.rs`).
The consequence is that a workflow declaring `type = "child-device"` matches nothing today.
The key keeps the three values of the `@type` vocabulary,
so the case that value is meant for stays expressible when it is designed:
a main device agent driving the workflows of a child device that cannot run its own agent.
Loading such a workflow is logged as a warning, since it will never be selected.

The registration data of any other target comes from the entity store over its REST API,
`GET /te/v1/entities/<topic-id>` (`crates/core/tedge_agent/src/http_server/entity_store.rs`),
which returns the entity's type and parent.
The request goes through the `tedge_http_ext` HTTP actor, which the agent now spawns,
built from `http.client_tls_config()` as the c8y mapper already does (`c8y/mapper.rs`),
and addressed at `http.client.host` and `http.client.port`.
The config and log managers already use that host to reach the main device
even when they run on the main device itself,
so this needs no new configuration and no new assumption about deployment.

The lookup is done per incoming command, with no cache.
Commands are rare events, so the round trip costs nothing that matters,
and not caching removes the need to invalidate anything when an entity is deregistered.

Alternative considered: an in-process client to the entity store actor on the main device,
falling back to REST on a child device.
Rejected — the entity store only runs on the main device (`agent.rs`),
so this is two code paths for one question,
and the in-process path needs the workflow builder (`agent.rs`)
to be constructed after the entity store (`agent.rs`).

Alternative considered: tracking registration messages inside the workflow actor
and adding a subscription per service through `DynSubscriptions`.
Rejected — it keeps a second copy of a relation the entity store already owns,
and it makes the agent's behaviour depend on the order in which retained messages replay.

### Map only the `control` command of a service to service commands

In `try_convert_data_message` (`converter.rs`),
a `Channel::CommandMetadata` message for the `control` operation of an `EntityType::Service`
is routed to the new service-command handling.
Every other command capability of a service goes through the existing per-operation mapping,
as it does for a device.

The `control` command of a service maps one to one to the `c8y_ServiceCommand` operation:
the `actions` of its capability are the `c8y_SupportedServiceCommands` of the service,
and one `c8y_ServiceCommand` operation is one `control` command.
So the mapper neither collects actions from several topics
nor decides per topic whether a command is an operation or a service command.

The mapper keeps the declared actions per service in memory,
to check an incoming operation against them,
and publishes `c8y_SupportedServiceCommands` on every `control` capability it receives,
using the existing inventory helper `inventory_update_message`
(`crates/extensions/c8y_mapper_ext/src/inventory.rs`).
`c8y_ServiceCommand` itself is registered once per service through the existing
`register_operation` path (`converter.rs`), which emits SmartREST `114`.

**Clearing is implemented, narrowly.**
An empty capability payload is ignored today with a warning (`converter.rs`).
For the `control` capability of a service it is handled:
the actions of the service are dropped and an empty
`c8y_SupportedServiceCommands` array is published.
Services appear and disappear at runtime, a container being the obvious case,
so without this the fragment drifts away from what the device can actually do.

Nothing else is removed on an empty payload,
so no supported operation is deregistered and no operation file is deleted.

### thin-edge's own services declare their own actions

`TEDGE_SERVICE_ACTIONS` (`crates/core/tedge_api/src/service_command.rs`) lists the actions each thin-edge service declares at startup:
`restart` alone, for the agent and for every `tedge-mapper-<x>`.

Under `tedge run all` no init unit manages a component,
so the agent declares `restart` alone — the one action which never reaches an init system —
and a mapper declares nothing.
There, and only there, a mapper clears its `control` capability:
a capability is retained, so it outlives the service that published it,
and moving a device to `tedge run all` would otherwise leave the action of the
previous deployment on show.

### One `control` command per service, with the action in the payload

A service declares its actions in the metadata of one command, `control`,
as `{"actions": [...]}` on `te/<service>/cmd/control`.
An action is requested as a `control` command whose payload names it,
`{"status", "action", "serviceName", "serviceType"}`.

This is how thin-edge commands already take a parameter.
The `log_upload` capability lists `{"types": [...]}`,
and a `log_upload` command names one of them in its `type` field.
`actions` and `action` follow the same pattern,
so the feature adds no new concept to the command metadata.

`serviceName` is the name the execution backend knows,
which is not derivable from the topic under a custom topic scheme.
`serviceType` selects the backend.
The field is named `action`, not `command` as in Cumulocity's `c8y_ServiceCommand`:
`command` is the cloud's word and stays inside the mapper.

The cost is that every action of every service of the device runs through one workflow,
the `control` workflow for services.
The shipped one runs any action through `tedge service`, custom actions included.
A different flow for one action is a branch on `${.payload.action}`
inside that workflow, not a workflow of its own.

Alternative considered: one command per action,
each declared with `{}` on `te/<service>/cmd/<action>`.
Rejected.
On a service, a `cmd/<action>` topic is then either an action of the service
or an ordinary operation such as `log_upload`,
and the mapper has to tell the two apart.
PR #4389 told them apart with a type in the capability payload, `{"@type": "service-action"}`.
A capability payload deciding what its command is would be a new concept in thin-edge,
and the distinction it draws is the one Cumulocity draws between operations and service commands,
so the thin-edge interface would follow the model of one cloud.

### Handle `c8y_ServiceCommand` as a native fragment

A `ServiceCommand` variant is added to `C8yDeviceControlOperation`
(`crates/core/c8y_api/src/json_c8y_deserializer.rs`) with its payload struct,
and an arm to `process_json_over_mqtt` (`converter.rs`)
that follows the shape of `forward_restart_request` (`converter.rs`):
resolve the entity with `EntityCache::try_get_by_external_id`
(`crates/extensions/c8y_mapper_ext/src/entity_cache.rs`),
build the command, publish it.

The thin-edge operation is always `control`,
and the `command` value of the operation becomes the `action` of the command, unchanged.
The value is validated against the action name rule `[A-Za-z][A-Za-z0-9_-]*`,
the same rule `tedge service` applies,
and it is checked against the actions the service has declared, case included.

The `serviceName` of the operation is taken as it comes.
Cumulocity holds one name per service:
the name the mapper published in the `102` message that created it (`converter.rs`),
which is the `name` of the entity registration message,
or the service segment of the topic identifier when the registration carries no name.

Deriving it from the entity instead was rejected.
Nothing in the registration data means "the name the backend knows":
neither the `name` nor the topic segment is guaranteed to be it,
so picking one would be a guess, and a silent one —
the operator would read one name on the operation
while a backend was asked for another.
Which name a service is reachable by is a contract on whoever registers it.

The shape of the name is checked, against the service name rule `tedge service` applies,
which lives in `tedge_api` next to the action name rule.
A name a backend could misread then fails the operation with that as its reason,
instead of failing later with the generic reason of a workflow step.
An absent name is reported the same way, Cumulocity always sending one.

The service type is checked the same way, and for the same reason.
It is checked after being resolved, not as it arrives,
since the type of the registration wins over the type of the payload
and both name a file under the service plugin directory.
All three rules — action, service name, service type — therefore live in `tedge_api`
and are applied by the mapper and by the CLI alike.

`OperationContext::update` (`crates/extensions/c8y_mapper_ext/src/operations/handlers/mod.rs`)
already publishes `501`-`506` on a topic derived from the entity,
and `C8yTopic::smartrest_response_topic`
(`crates/core/c8y_api/src/smartrest/topic.rs`) already maps a service to `c8y/s/us/<service-xid>`.
A status handler for service commands is added alongside the existing ones.
It is selected from the topic alone, the `control` operation of a service,
so reporting a status needs no state kept by the mapper.

Alternative considered: a shipped custom operation handler file with `on_fragment = "c8y_ServiceCommand"`.
Rejected in 0011: the feature also needs the `c8y_SupportedServiceCommands` fragment,
and operation files for services are not reloaded dynamically.

### Take each naming rule from how the name is used

An action name is `[A-Za-z][A-Za-z0-9_-]*`:
letters, digits, `_` and `-`, starting with a letter.
It is the largest set of characters left once every step a name takes has had its say:

- the name is passed as one argument to an init tool or to a service plugin
  → no leading `-`, and no shell metacharacter;
- the name is a key of `[init]` in `system.toml`, so it has to be a TOML bare key,
  which accepts only letters, digits, `_` and `-` → this is what leaves `.` and `@` out.

A service type has one constraint of its own:
it names a file under the plugin directory, so it holds no `/` and is neither `.` nor `..`.
The rest of its rule is lowercase letters, digits, `_` and `-`,
so that one spelling of a type names one plugin.

A service name is derived the same way, and two steps have something to say about it:

- a name is not empty, an empty argument naming no service at all;
- a name does not start with `-`.
  The `{}` of an `[init]` template is replaced one argv element at a time
  (`general_manager.rs`), so a name holding a space cannot become two arguments,
  but `systemctl restart --now` and `rc-service --now restart`
  read that argument as an option of the tool, and the name stops being a name.

Nothing else is refused, the length included.
A service name is the one the device registered the service under,
`name` being a plain key of the registration payload (`entity.rs`) with no rule of its own,
so a rule here refuses a service Cumulocity shows.

Alternative considered: a whitelist of the characters a real service name needs,
`[A-Za-z0-9_.@-]`.
Rejected — it refuses a name the device registered, a systemd unit name being free to hold `:`,
and adding a character each time such a name turns up leaves the next one refused.
What it would guard against is a backend running a shell,
an `[init]` template being an argv list the user writes,
and that is the exposure the config and log plugins already carry:
both pass a type from the cloud under `sudo` to a plugin shipped as a `/bin/sh` script
(`tedge_config_manager/src/plugin.rs`), with no character rule at all.
Quoting is the backend's job.

### Add custom actions as plain keys of `[init]`

The templates in `system.toml` are called **actions**, not commands,
to keep them apart from thin-edge commands.

A custom action is written as a plain key of the existing `[init]` table:

```toml
[init]
name = "systemd"
reload = ["/usr/bin/systemctl", "reload", "{}"]
```

`[init]` today is one string, `name`, plus seven argv templates
(`crates/common/tedge_config/src/system_toml/services.rs`).
A custom action is the same kind of value as `restart` or `is_active`,
so it belongs in the same table.
`InitConfig` gains a map that collects the keys beyond the known ones,
and `InitConfigToml`'s `deny_unknown_fields` (`services.rs`) is dropped,
since serde does not combine `deny_unknown_fields` with `flatten`.

Every key is an action except `name` and `is_available`, which describe the init system
rather than a service (`InitConfig::action`, `services.rs`).

The cost is that a misspelled known key, `restrat` instead of `restart`,
is now read as a custom action rather than rejected,
and the device silently falls back to the systemd default for `restart`.
Three things make that visible:
the actions parsed from `[init]` are logged at start-up,
`tedge service` lists the actions `[init]` defines when it rejects an unsupported one,
and the fallback to a default template is logged.

Alternative considered: a sub-table, `[init.actions]` or a top-level `[actions]`,
which would preserve `deny_unknown_fields`.
Rejected — it splits one concept across two places,
leaving users to remember that `restart` goes in `[init]` and `reload` in `[actions]`,
and it is not what 0011 describes.

`SystemServiceManager` (`manager.rs`) gains a method to run an action by name,
which the existing per-action methods can be expressed in terms of.
Execution stays argv-based in `GeneralServiceManager` (`general_manager.rs`).

### `tedge service` follows the diag-plugin precedent

A `Service` variant is added to `TEdgeOpt` (`crates/core/tedge/src/cli/mod.rs`)
with a command implementing `Command` (`crates/core/tedge/src/command.rs`).
It obtains the service manager the same way `tedge connect` does,
`tedge_system_services::service_manager(config.root_dir())` (`cli/connect/cli.rs`).

Exit codes follow `tedge diag collect` (`crates/core/tedge/src/cli/diag/collect.rs`),
which already uses `0` for success and `2` for "this plugin skipped it":
`0` success, `2` command not supported for this service type, other non-zero failure.
The plugin's own `2` is propagated unchanged.

The plugin directories are a new key of the existing `service` table,
`service.plugin_paths` (`crates/common/tedge_config/src/tedge_toml/tedge_config.rs`),
defaulting to `/usr/share/tedge/service-plugins`.
It is a `TemplatesSet`, a comma-separated list of directories,
the same name and shape as `diag.plugin_paths`, `log.plugin_paths`
and `configuration.plugin_paths`,
so an administrator can add a directory of their own next to the shipped one.
A plugin is picked by its exact name, the service type,
from the first directory that holds one, as it is for the other three.
Packaging adds the directory to `configuration/package_manifests/nfpm.tedge.yaml`
with mode `0755`, following the `log-plugins` and `config-plugins` entries.
It must **not** be added to the chown list in `package_scripts/tedge/preinst`,
which is what keeps it out of the `tedge` user's reach.

No sudoers change is needed.
`tedge service` runs the plugin while already root, so the plugin never goes through sudo,
and `/usr/bin/tedge` is already covered by the shipped rule.

## Risks / Trade-offs

- The mapper handles an empty payload on the `control` capability of a service,
  and ignores it on every other capability
  → the difference is deliberate and sits in one place, the `control` branch of
  `try_convert_data_message`.
- A service can clear its `control` capability, leaving no action
  → `c8y_ServiceCommand` stays registered as a supported operation, so Cumulocity still
  offers the operation with no action to pick.
  Deregistering a supported operation is out of scope here.
- Keying the workflow registry by `(EntityType, OperationType)` touches every call site in
  `supervisor.rs` and the agent's routing
  → the default is `device`, so every existing workflow keeps its current key and behaviour;
  covered by the existing workflow tests.
- The agent now receives every entity's command messages, including those of other devices
  → commands are rare, low-volume events, not telemetry, and every non-matching message is
  dropped after one lookup. The gain is that scoping no longer assumes a topic structure.
- Deciding whether to act depends on the entity store being reachable
  → a failed lookup means the agent cannot tell whether the command is its own,
  so it does not act and logs the failure, leaving the command for a retry rather than
  driving a state machine it may not own.
  On a child device this makes service commands depend on the connection to the main device,
  which is the same dependency the file transfer service already has.
- `tedge service` runs as root and takes an action name that originates in the cloud
  → every argument is validated before any backend is selected, and all execution is
  argv-based, so a value cannot become extra arguments or a shell fragment.

## Migration Plan

Nothing to migrate.
Every new field is optional and defaults to today's behaviour:
a workflow without `type` is a device workflow,
`system.toml` with no extra `[init]` keys supports exactly the actions it does today,
and a device without a service plugin directory simply has no non-default service types.

The one behaviour that is not preserved is that `[init]` no longer rejects an unknown key.
A `system.toml` that fails to load today will load after this change,
with the offending key read as a custom action.
