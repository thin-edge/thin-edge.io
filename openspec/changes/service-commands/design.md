## Context

The domain background, the alternatives, and the reasons behind the user-facing shape of this feature
are in `design/decisions/0011-service-commands.md`.
This document does not repeat them.
It records the engineering decisions needed to build the feature in the current code base.

The relevant current state:

- **Workflow definitions are keyed by operation name only.**
  `TomlOperationWorkflow` (`crates/core/tedge_api/src/workflow/toml_config.rs:33`)
  has `operation` plus flattened handlers and states.
  `WorkflowSupervisor.workflows` is a `HashMap<OperationType, WorkflowVersions>`
  (`crates/core/tedge_api/src/workflow/supervisor.rs:12`),
  and `WorkflowRepository.definitions` is keyed by operation name
  (`crates/core/tedge_agent/src/operation_workflows/persist.rs:42`).
  Nothing in the engine knows which entity type a workflow is for.
- **The agent only subscribes to its own device's commands.**
  `WorkflowActorBuilder::subscriptions` (`crates/core/tedge_agent/src/operation_workflows/builder.rs:166`)
  builds `te/device/<own>///cmd/+/+`.
  `WorkflowActor::process_mqtt_message` (`crates/core/tedge_agent/src/operation_workflows/actor.rs:163`)
  then discards the entity part of the parsed topic, because there is only one possible target.
- **The c8y mapper turns a capability message into an operation file.**
  `Channel::CommandMetadata` is handled in `try_convert_data_message`
  (`crates/extensions/c8y_mapper_ext/src/converter.rs:1132`),
  which maps a known command to a c8y fragment and calls `register_operation`
  (`converter.rs:1328`), which writes a file under `/etc/tedge/operations/c8y/<xid>/`
  and then republishes SmartREST `114`.
  An empty capability payload is ignored with a warning (`converter.rs:1134`, issue #2739),
  so clearing a capability does nothing today.
- **The init system is abstracted, but with a fixed set of commands.**
  `InitConfig` (`crates/common/tedge_config/src/system_toml/services.rs:3`)
  has exactly `name`, `is_available`, `restart`, `stop`, `start`, `enable`, `disable`, `is_active`,
  each an argv template with a `{}` placeholder.
  `InitConfigToml` (`services.rs:16`) uses `deny_unknown_fields`.
  `SystemServiceManager` (`crates/common/tedge_system_services/src/manager.rs:11`)
  exposes one method per command, and `GeneralServiceManager` runs them argv-based, without a shell
  (`managers/general_manager.rs:232`).
- **`/usr/bin/tedge` is already a sudoers entry**
  (written by `configuration/package_scripts/tedge/preinst:89`),
  so a new `tedge` subcommand needs no packaging change to run as root.

## Goals / Non-Goals

**Goals:**

- One executor per device, with third parties plugged in below the command state machine.
- A workflow can be selected by target entity type, so `restart` for a device
  and `restart` for a service are separate workflows.
- The c8y mapper handles `c8y_ServiceCommand` natively, without an operation file per command.
- `tedge service` is usable on its own, as an init-system-agnostic service wrapper.

**Non-Goals:**

- Discovering service capabilities from the agent (the pull model in 0011's future work).
- Filtering which declared capabilities are exposed to the cloud.
- Commands addressed to services of other devices.
- Reworking how workflow files are named on disk beyond what this feature needs.

## Decisions

### Scope the workflow registry by entity type, not by a mangled operation name

`TomlOperationWorkflow` gains an optional `type` field holding an `EntityType`,
defaulting to `device`.
It must be declared **before** the flattened `handlers` and `states` fields,
otherwise serde folds it into the state map.

The registry key changes from `OperationType` to `(EntityType, OperationType)`
in `WorkflowSupervisor.workflows`, and the same pair is threaded through
`register_custom_workflow`, `unregister_custom_workflow`, `capability_message`,
`use_current_version`, `apply_external_update` and `get_action` (`supervisor.rs:31`-`:206`).

Alternative considered: encoding the type into the operation name, for example `service/restart`.
Rejected — the operation name appears in the MQTT topic and in the cloud mapping,
so overloading it would leak the scoping into the wire format.

`WorkflowRepository.definitions` (`persist.rs:42`) is keyed the same way,
and gets the same treatment.

Two methods do not take the entity type.

`get_action` keeps its signature.
A command under execution is identified by its operation name and its `@version`,
and the version is the digest of the workflow definition (`persist.rs:483`),
of which `type` is part.
So two workflows sharing an operation name never share a version,
and the workflow of a running command is found without knowing the entity type.
This also means a command resumed after an agent restart needs nothing more than
what is already persisted with it, which matters for the agent self-restart case.

`deregistration_message` is only sent for a device workflow.
The capabilities of a service are declared by that service,
so a deleted service workflow file clears nothing on the device's topics.
For the same reason `capability_messages` is filtered on the entity type of the target.

0011 leaves open how two workflows named `restart` can live in `/etc/tedge/operations/`
when the file name is `restart.toml` in both cases.
Reading the code, the question does not arise: the file name is never used as the operation name.
`load_operation_workflow` takes the name from the parsed content, `workflow.operation` (`persist.rs:132`),
`read_operation_workflow` uses the path only in error messages (`persist.rs:478`),
and removal finds the entry by comparing paths (`persist.rs:245`).
So a file may be named anything, and the only thing a user has to do
is set `type = "service"` inside it.

Alternative considered: a naming convention such as `service-restart.toml`,
or a per-type subdirectory `operations/service/restart.toml`.
Both are unnecessary once the keys carry the entity type,
and the subdirectory would additionally require changing `is_user_defined` (`persist.rs:211`),
which expects the file's parent to be exactly the workflows directory,
along with the inotify watch that only covers that directory.

### Subscribe to every entity's commands, and ask the entity store who the target is

A topic filter cannot express "services of my device" under a custom topic scheme,
because the device-service relation is carried by the registration message's `@parent`,
not by the topic structure.
So the scoping is not done by the subscription; it is done when a command arrives.

The subscription in `WorkflowActorBuilder::subscriptions` (`builder.rs:166`) is widened to
`EntityFilter::AnyEntity` with `ChannelFilter::AnyCommand`, keeping the own-service signal filter.
`WorkflowActor::process_mqtt_message` currently drops the entity from the parsed topic (`actor.rs:163`);
it will keep it and decide from the entity's registration data whether to act:

- the agent's own device: act, as today;
- an entity of type `service` whose parent is the agent's own device: act;
- anything else: ignore.

The registration data comes from the entity store over its REST API,
`GET /te/v1/entities/<topic-id>` (`crates/core/tedge_agent/src/http_server/entity_store.rs:167`),
which returns the entity's type and parent.
The agent already holds `tedge_http_host` and `tedge_http_protocol` for the file transfer service,
and the config and log managers already use that host to reach the main device
even when they run on the main device itself,
so this needs no new configuration and no new assumption about deployment.

The lookup is done per incoming command, with no cache.
Commands are rare events, so the round trip costs nothing that matters,
and not caching removes the need to invalidate anything when an entity is deregistered.

Alternative considered: an in-process client to the entity store actor on the main device,
falling back to REST on a child device.
Rejected — the entity store only runs on the main device (`agent.rs:424`),
so this is two code paths for one question,
and the in-process path needs the workflow builder (`agent.rs:317`)
to be constructed after the entity store (`agent.rs:433`).

Alternative considered: tracking registration messages inside the workflow actor
and adding a subscription per service through `DynSubscriptions`.
Rejected — it keeps a second copy of a relation the entity store already owns,
and it makes the agent's behaviour depend on the order in which retained messages replay.

### Treat every command capability of a service entity as a service command

In `try_convert_data_message` (`converter.rs:1132`),
a `Channel::CommandMetadata` message whose target entity is an `EntityType::Service`
is routed to the new service-command handling
instead of the existing per-operation mapping.
So a service declaring `cmd/restart` declares the service command `RESTART`,
not the device operation `c8y_Restart`.

The mapper keeps the declared command names per service in memory
and republishes the whole `c8y_SupportedServiceCommands` array on every change,
using the existing inventory helper `inventory_update_message`
(`crates/extensions/c8y_mapper_ext/src/inventory.rs:82`).
`c8y_ServiceCommand` itself is registered once per service through the existing
`register_operation` path (`converter.rs:1328`), which emits SmartREST `114`.

This changes today's behaviour for a service that declares a command capability.
In practice thin-edge's own services do not declare capabilities on their service topics today,
so the exposure is small, but it is a behaviour change and is called out as a risk.

**Withdrawal is implemented, narrowly.**
An empty capability payload is ignored today with a warning
(`converter.rs:1134`, issue #2739).
For command metadata on a service entity it is handled:
the command is dropped from the service's set and the reduced
`c8y_SupportedServiceCommands` array is published.
Services appear and disappear at runtime — a container is the obvious case —
so without this the fragment drifts away from what the device can actually do.

The rest of #2739 is left alone.
Nothing outside the service-command set is removed on an empty payload,
so no supported operation is deregistered and no operation file is deleted.

### The command payload does not name the action

0011 gives the command payload as
`{"status", "command", "serviceName", "serviceType"}`.
The `command` field is dropped.

thin-edge names an operation in the topic, not in the payload.
`RestartCommandPayload` (`crates/core/tedge_api/src/commands.rs:681`) carries
`status` and `log_path`;
`SoftwareUpdateCommandPayload` (`commands.rs:420`) carries
`status`, `update_list`, `failures` and `log_path`.
Neither repeats its own operation name, and no other command payload does either.
0011's own workflow example already reads the action from `${.topic.operation}`,
never from the payload, so the field has no reader.
Keeping it would also raise a question with no useful answer:
which one wins when the topic says `restart` and the payload says `pause`.

The payload is therefore `{"status", "serviceName", "serviceType"}`.
`serviceName` stays because it is the name the execution backend knows,
which is not derivable from the topic under a custom topic scheme.
`serviceType` stays because it selects the backend.

The `command` field exists in 0011 because it mirrors Cumulocity's `c8y_ServiceCommand`
fragment, which carries `serviceType`, `serviceName` and `command`.
thin-edge does not need to follow Cumulocity's shape;
`command` is the cloud's word and stays inside the mapper.

Nothing is lost with the field, because an action name is a single lowercase token,
`[a-z][a-z0-9_]+`.
MQTT topic names are case-sensitive and do accept spaces,
so this is a restriction thin-edge puts on itself:
a name such as `do something` or `doSomething` is out of scope,
and is rejected by the mapper and by `tedge service` alike.
Cumulocity uppercases a command name by convention,
so lowercasing the value it sends gives back the declared action name exactly,
and there is no original spelling left to carry.

This needs a matching edit in 0011.

### Handle `c8y_ServiceCommand` as a native fragment

A `ServiceCommand` variant is added to `C8yDeviceControlOperation`
(`crates/core/c8y_api/src/json_c8y_deserializer.rs:34`) with its payload struct,
and an arm to `process_json_over_mqtt` (`converter.rs:509`)
that follows the shape of `forward_restart_request` (`converter.rs:823`):
resolve the entity with `EntityCache::try_get_by_external_id`
(`crates/extensions/c8y_mapper_ext/src/entity_cache.rs:377`),
build the command, publish it.

The difference from the existing operations is that the thin-edge operation name is not fixed:
it comes from the lowercased `command` value in the payload.
So the name is validated against the action name rule `[a-z][a-z0-9_]+`,
the same rule `tedge service` applies, before it is used to build a topic,
and it is checked against the set of commands the service has declared.
Both the mapper and the CLI need that rule, so it lives in `tedge_api` and is used by both.

Status reporting needs no new mapping.
`OperationContext::update` (`crates/extensions/c8y_mapper_ext/src/operations/handlers/mod.rs:77`)
already publishes `501`-`506` on a topic derived from the entity,
and `C8yTopic::smartrest_response_topic`
(`crates/core/c8y_api/src/smartrest/topic.rs:21`) already maps a service to `c8y/s/us/<service-xid>`.
A handler module for the new operation type is added alongside the existing ones.

Alternative considered: a shipped custom operation handler file with `on_fragment = "c8y_ServiceCommand"`.
Rejected in 0011: the feature also needs the `c8y_SupportedServiceCommands` fragment,
and operation files for services are not reloaded dynamically.

### Add custom actions as plain keys of `[init]`

The templates in `system.toml` are called **actions**, not commands,
to keep them apart from the thin-edge commands carried on `cmd/<action>` topics.

A custom action is written as a plain key of the existing `[init]` table:

```toml
[init]
name = "systemd"
reload = ["/usr/bin/systemctl", "reload", "{}"]
```

`[init]` today is one string, `name`, plus seven argv templates
(`crates/common/tedge_config/src/system_toml/services.rs:3`).
A custom action is the same kind of value as `restart` or `is_active`,
so it belongs in the same table.
`InitConfig` gains a map that collects the keys beyond the known ones,
and `InitConfigToml`'s `deny_unknown_fields` (`services.rs:17`) is dropped,
since serde does not combine `deny_unknown_fields` with `flatten`.
`name` and the predicate templates stay reserved and are not dispatchable as actions.

The cost is that a misspelled known key, `restrat` instead of `restart`,
is now read as a custom action rather than rejected,
and the device silently falls back to the systemd default for `restart`.
Three things make that visible:
the actions parsed from `[init]` are logged at start-up,
`tedge service` lists the known actions when it rejects an unsupported one,
and the fallback to a default template is logged.

Alternative considered: a sub-table, `[init.actions]` or a top-level `[actions]`,
which would preserve `deny_unknown_fields`.
Rejected — it splits one concept across two places,
leaving users to remember that `restart` goes in `[init]` and `reload` in `[actions]`,
and it is not what 0011 describes.

`SystemServiceManager` (`manager.rs:11`) gains a method to run an action by name,
which the existing per-action methods can be expressed in terms of.
Execution stays argv-based in `GeneralServiceManager` (`general_manager.rs:232`).

### `tedge service` follows the diag-plugin precedent

A `Service` variant is added to `TEdgeOpt` (`crates/core/tedge/src/cli/mod.rs:97`)
with a command implementing `Command` (`crates/core/tedge/src/command.rs:63`).
It obtains the service manager the same way `tedge connect` does,
`tedge_system_services::service_manager(config.root_dir())` (`cli/connect/cli.rs:52`).

Exit codes follow `tedge diag collect` (`crates/core/tedge/src/cli/diag/collect.rs:71`),
which already uses `0` for success and `2` for "this plugin skipped it":
`0` success, `2` command not supported for this service type, other non-zero failure.
The plugin's own `2` is propagated unchanged.

The plugin directory is a new key of the existing `service` table,
`service.plugin_dir` (`crates/common/tedge_config/src/tedge_toml/tedge_config.rs:1327`),
defaulting to `/usr/share/tedge/service-plugins`.
It is a single directory, not a search path like `log.plugin_paths`,
because a plugin is picked by its exact name, the service type,
so a second directory would only raise the question of which plugin wins.
Packaging adds the directory to `configuration/package_manifests/nfpm.tedge.yaml`
with mode `0755`, following the `log-plugins` and `config-plugins` entries.
It must **not** be added to the chown list in `package_scripts/tedge/preinst:102`,
which is what keeps it out of the `tedge` user's reach.

No sudoers change is needed.
`tedge service` runs the plugin while already root, so the plugin never goes through sudo,
and `/usr/bin/tedge` is already covered by the shipped rule.

## Risks / Trade-offs

- Handling an empty capability payload for service entities only means the mapper treats
  the same message differently depending on the target entity type
  → the difference is deliberate and sits in one place, the `EntityType::Service` branch of
  `try_convert_data_message`. It also removes one case from #2739 rather than adding to it.
- A service can withdraw every command it declared, leaving an empty set
  → the mapper publishes an empty `c8y_SupportedServiceCommands` array,
  which is what tells Cumulocity to stop offering the buttons.
  `c8y_ServiceCommand` stays registered as a supported operation,
  since deregistering it is the part of #2739 this change does not solve.
- A service that declares `cmd/restart` today would get `c8y_Restart`, and will now get
  `c8y_ServiceCommand` with `RESTART`
  → no thin-edge service declares command capabilities today, and the new behaviour is
  what a service action means in Cumulocity. Called out in the release notes.
  The old mapping is not kept in parallel: a service declaring both `c8y_Restart` and
  `c8y_ServiceCommand` would show two buttons for one action, which helps nobody migrate.
- Every `cmd/<action>` topic of a service becomes an action offered in the cloud,
  and a service owner has no way to declare an action locally without exposing it.
  A service declaring both `cmd/restart` and `cmd/collect_measurements` gets both
  `RESTART` and `COLLECT_MEASUREMENTS` in `c8y_SupportedServiceCommands`,
  even if only `restart` was meant for the cloud
  → this is the filtering gap 0011 records as future work, and it is a Non-Goal here.
  The change makes it more visible, because before this change a service's `cmd/*` topics
  reached the cloud only through the fixed set of known operations.
  Whatever the filter turns out to be, it belongs in the mapper, not in the thin-edge
  interface, so the capability declaration in `service-commands` does not need to change.
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
  → the name is validated against `[a-z][a-z0-9_]+` before any backend is selected, and all
  execution is argv-based, so the value cannot become extra arguments or a shell fragment.

## Migration Plan

Nothing to migrate.
Every new field is optional and defaults to today's behaviour:
a workflow without `type` is a device workflow,
`system.toml` with no extra `[init]` keys supports exactly the actions it does today,
and a device without a service plugin directory simply has no non-default service types.

The one behaviour that is not preserved is that `[init]` no longer rejects an unknown key.
A `system.toml` that fails to load today will load after this change,
with the offending key read as a custom action.

## Open Questions

- **Where the declared-command set lives in the mapper.**
  An in-memory map on the converter is the smallest change,
  but it makes the `c8y_SupportedServiceCommands` fragment depend on retained messages replaying.
  That is how 0011 describes it, and it is acceptable, but worth confirming
  against how the mapper handles other derived inventory fragments.
- **Whether `action` is the final term.**
  `action` is used here for the `system.toml` templates and for the `cmd/<action>` topic segment,
  while the command payload and the Cumulocity fragment keep the word `command`.
  The choice between `action` and `operation` is still being discussed.
