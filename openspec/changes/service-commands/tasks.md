## 1. Workflow engine: scope workflows by entity type

- [x] 1.1 Add an optional `type` field holding an `EntityType` to `TomlOperationWorkflow` (`crates/core/tedge_api/src/workflow/toml_config.rs:33`), declared **before** the flattened `handlers` and `states` fields so serde does not fold it into the state map; default to `device`
- [x] 1.2 Carry the field through `TryFrom<TomlOperationWorkflow> for OperationWorkflow` (`toml_config.rs:225`), `OperationWorkflow` (`workflow/mod.rs:60`), `try_new` (`mod.rs:248`), `built_in` (`mod.rs:308`) and `ill_formed` (`mod.rs:343`)
- [x] 1.3 Change `WorkflowSupervisor.workflows` (`workflow/supervisor.rs:12`) to key on `(EntityType, OperationType)` and update `register_custom_workflow`, `unregister_custom_workflow`, `capability_message`, `use_current_version` and `apply_external_update` (`supervisor.rs:31`-`:206`). `get_action` keeps its signature: a command under execution is found by its operation and `@version`, which is the digest of the workflow definition
- [x] 1.3b Filter `capability_messages` on the entity type, so a service workflow does not declare a capability on the device's topics
- [x] 1.4 Change `WorkflowRepository.definitions` (`crates/core/tedge_agent/src/operation_workflows/persist.rs:42`) to the same key, and update `load_operation_workflow` (`persist.rs:132`) and `remove_operation_workflow` (`persist.rs:245`)
- [x] 1.5 Unit-test TOML parsing of `type` (present, absent, invalid value) next to the existing cases in `toml_config.rs:699-930`
- [x] 1.6 Unit-test that a device `restart` workflow and a service `restart` workflow coexist and that neither drives the other's commands, and that an existing workflow without `type` keeps behaving as before

## 2. Agent: react to commands addressed to its own device's services

- [x] 2.1 Widen `WorkflowActorBuilder::subscriptions` (`crates/core/tedge_agent/src/operation_workflows/builder.rs:166`) to `EntityFilter::AnyEntity` with `ChannelFilter::AnyCommand`, keeping the own-service signal filter
- [x] 2.2 Add an entity lookup client to the workflow actor that calls `GET /te/v1/entities/<topic-id>` (`crates/core/tedge_agent/src/http_server/entity_store.rs:167`) using the existing `http.client` host and port. The client goes through the `tedge_http_ext` HTTP actor, which the agent now spawns, so the TLS setup of `http.client_tls_config()` is reused as the c8y mapper does
- [x] 2.3 Keep the entity in `WorkflowActor::process_mqtt_message` (`operation_workflows/actor.rs:163`) and act only when the target is the agent's own device, or a `service` whose parent is the agent's own device; ignore anything else with a log line
- [x] 2.3b Classify the target for the workflow lookup, replacing the `EntityType::MainDevice` passed everywhere by section 1. **Decided**: the device the agent runs on is classified as `device` with no lookup at all, and every other target is classified with the `@type` the entity store reports, with no fallback. Classifying the own device by its reported `@type` was rejected: an agent on a child device reports itself as `child-device` and would need a `(child-device, operation)` to `(device, operation)` fallback, its own device commands would depend on the entity store being reachable and on the device being registered there, and under a custom topic scheme it is not registered at all (`EntityStore::with_main_device` only ever holds `device/main//`, `agent.rs:409`). A workflow declaring `type = "child-device"` therefore matches nothing yet
- [x] 2.4 On a failed lookup, do not act and log the failure, so the command is left for a retry rather than half-driven
- [x] 2.5 Use `(entity_type, operation)` for the workflow lookup in `process_command_message` (`actor.rs:223`) and `apply_external_update` (`persist.rs:401`)
- [x] 2.6 Test: a command for a service of the own device is driven; a command for a service of another device is ignored; a command for an unregistered entity is ignored; a lookup failure leaves the command untouched
- [x] 2.7 Test that the same behaviour holds under a custom topic scheme, where the parent relation is only in the entity store

## 3. `system.toml`: custom action templates

- [x] 3.1 Add a map to `InitConfig` (`crates/common/tedge_config/src/system_toml/services.rs:3`) collecting the `[init]` keys beyond the known ones, and drop `deny_unknown_fields` from `InitConfigToml` (`services.rs:17`)
- [x] 3.2 Keep `name` and the state-query templates reserved and not dispatchable as actions
- [x] 3.3 Log the actions parsed from `[init]` when the configuration is loaded, and log when an action falls back to a default template, so a misspelled key stays discoverable
- [x] 3.4 Add a method to `SystemServiceManager` (`crates/common/tedge_system_services/src/manager.rs:11`) that runs an action by name, and express the existing per-action methods in terms of it
- [x] 3.5 Implement it in `GeneralServiceManager` (`managers/general_manager.rs:14`) reusing `ExecCommand::try_new_with_placeholder` (`general_manager.rs:103`), keeping execution argv-based
- [x] 3.6 Test: a custom `reload` template is executed; an undefined action is reported as unsupported; a reserved key is not dispatchable; a `system.toml` with no extra keys behaves exactly as today

## 4. `tedge service` CLI

- [x] 4.1 Add a `Service` variant to `TEdgeOpt` (`crates/core/tedge/src/cli/mod.rs:97`), the module declaration (`mod.rs:22`) and the `BuildCommand` arm (`mod.rs:196`)
- [x] 4.2 Define `tedge service <action> <service-name> [--service-type <type>]`, `--service-type` defaulting to `service`
- [x] 4.3 Validate the arguments before selecting any backend: action matches `[a-z][a-z0-9_]+` and is bounded in length, service name matches `[A-Za-z0-9_.@-]+` with no leading `-`, service type matches `[a-z0-9_-]+`. Put the action name rule in `tedge_api` so the mapper uses the same one (7.5)
- [x] 4.4 Dispatch the `service` type to the init-system backend via `tedge_system_services::service_manager(config.root_dir())`, as `tedge connect` does (`cli/connect/cli.rs:52`)
- [x] 4.5 Dispatch any other type to `<service-plugin-dir>/<type> <action> <service-name>`, spawned argv-based following `tedge diag collect` (`cli/diag/collect.rs:159`)
- [x] 4.6 Map exit codes: `0` success, `2` action not supported for this service type, other non-zero failure with the backend's stderr as the reason; propagate a plugin's `2` unchanged
- [x] 4.7 List the known actions in the error when an action is rejected as unsupported
- [x] 4.8 Add a `service.plugin_dir` config key to the existing `service` table (`crates/common/tedge_config/src/tedge_toml/tedge_config.rs:1327`), defaulting to `/usr/share/tedge/service-plugins`
- [x] 4.9 Test: default type restarts through the init system; a custom type runs its plugin; a missing plugin file, a rejected argument, and each exit code path

## 5. Shipped service command workflows

- [ ] 5.1 Ship `start`, `stop` and `restart` workflows with `type = "service"` under `crates/core/tedge_agent/src/resources/`, whose execution step runs `sudo -n tedge service ${.topic.operation} ${.payload.serviceName} --service-type ${.payload.serviceType}`
- [ ] 5.2 Give each workflow a timeout so a backend that never returns ends as `failed`
- [ ] 5.3 Implement the agent self-restart case with the existing pattern — `action = "restart-agent"` then `await-agent-restart` (`crates/extensions/tedge_config_manager/src/resources/config_update.toml:26-39`) — so the step is not re-executed on resume
- [ ] 5.4 Reject `stop` addressed to tedge-agent or to a cloud mapper with a reason naming why
- [ ] 5.5 Test: a successful run, a non-zero exit, a timeout, the agent self-restart completing exactly once, and both rejected `stop` cases

## 6. c8y mapper: capabilities to Cumulocity

- [x] 6.1 In `try_convert_data_message` (`crates/extensions/c8y_mapper_ext/src/converter.rs:1132`), route `Channel::CommandMetadata` for an `EntityType::Service` target to the new service-command handling instead of the per-operation mapping
- [x] 6.2 Keep the declared action names per service and register `c8y_ServiceCommand` once per service through `register_operation` (`converter.rs:1328`), which emits SmartREST `114`. `register_operation` is called on every newly declared action and only reports to the cloud when the set of supported operations actually changes, so a failed registration is retried by the next action
- [x] 6.3 Publish the uppercased set as `c8y_SupportedServiceCommands` with `inventory_update_message` (`src/inventory.rs:82`), republishing the whole array on every change
- [x] 6.4 Handle the empty payload for command metadata on service entities: drop the action from the set and publish the reduced array, without touching anything else covered by issue #2739. `c8y_ServiceCommand` is left registered as a supported operation, even when the array becomes empty
- [x] 6.4b Do not declare a capability whose action name breaks the rule of 4.3, and log why: Cumulocity would show a command that no lowercasing could route back to that capability topic
- [x] 6.5 Test: standard actions uppercased; a custom action passed through; one action withdrawn; all actions withdrawn giving an empty array; the set rebuilt from retained messages after a mapper restart

## 7. c8y mapper: `c8y_ServiceCommand` operation

- [ ] 7.1 Add a `ServiceCommand` variant and payload struct to `C8yDeviceControlOperation` (`crates/core/c8y_api/src/json_c8y_deserializer.rs:34`)
- [ ] 7.2 Add the arm to `process_json_over_mqtt` (`converter.rs:509`) and a forward function following `forward_restart_request` (`converter.rs:823`), resolving the entity with `EntityCache::try_get_by_external_id` (`src/entity_cache.rs:377`)
- [ ] 7.3 Build the command topic from the lowercased `command` value, and do not copy it into the thin-edge payload; publish `{"status": "init", "serviceName", "serviceType"}`
- [ ] 7.4 Take `serviceName` from the resolved entity topic identifier, and `serviceType` from the registration data, falling back to the operation payload and then to the default type `service`
- [ ] 7.5 Fail the cloud operation, publishing no command, when the entity cannot be resolved, when the action is not declared (compared case-insensitively), or when the lowercased `command` value does not match the action name rule `[a-z][a-z0-9_]+` from 4.3
- [ ] 7.6 Add the `OperationType` variant (`crates/core/tedge_api/src/mqtt_topics.rs:738`), the `CumulocitySupportedOperations` mapping (`c8y_api/src/smartrest/smartrest_serializer.rs:184`), the `to_c8y_operation` entry and a status handler module alongside the existing ones (`src/operations/handlers/`), plus the topic filter in `OperationHandler::topic_filter` (`src/operations/handler.rs:226`)
- [ ] 7.7 Test: a RESTART operation becoming a `restart` command; type from registration winning over the payload; the default type for an untyped service; a display name in the payload being ignored; each rejection case; `501`-`506` reported on the service's topic

## 8. Packaging

- [x] 8.1 Add `/usr/share/tedge/service-plugins/` to `configuration/package_manifests/nfpm.tedge.yaml` with mode `0755`, following the `log-plugins` and `config-plugins` entries
- [x] 8.2 Confirm it is **not** added to the chown list in `configuration/package_scripts/tedge/preinst:102`, so it stays out of the `tedge` user's reach
- [x] 8.3 Confirm no sudoers change is needed: `/usr/bin/tedge` is already covered (`preinst:89`) and the plugin is run by an already-root process

## 9. Documentation

- [ ] 9.1 Document the service command interface: the per-action capability topics, the command payload, that the action is named by the topic only, and the action name rule `[a-z][a-z0-9_]+`
- [ ] 9.2 Document the workflow `type` field, including that the file name is free and only `type = "service"` distinguishes a service workflow
- [ ] 9.3 Document `tedge service`, its exit codes, and the service plugin contract with the example plugin from 0011
- [ ] 9.4 Document custom action templates in `[init]`, and note in the release notes that `[init]` no longer rejects unknown keys
- [ ] 9.5 Note in the release notes that a service declaring `cmd/restart` now maps to `c8y_ServiceCommand` rather than `c8y_Restart`

## 10. Integration tests

- [ ] 10.1 Add `tests/RobotFramework/tests/cumulocity/service_command/` covering an init-managed service end to end: capability declaration, `c8y_SupportedServiceCommands`, a RESTART operation, and the `506` result
- [ ] 10.2 Cover a service plugin type end to end with a test plugin, including a custom action
- [ ] 10.3 Cover the rejections: `stop` of tedge-agent, `stop` of a cloud mapper, and an undeclared action
- [ ] 10.4 Cover the agent self-restart case

## 11. Update the design decision record

`design/decisions/0011-service-commands.md` was written before these decisions.
Each item below is a place where 0011 no longer matches. Keep it in its own commit.

- [ ] 11.1 Command payload: remove the `action` field from the JSON field list and from both end-to-end examples (0011 names it `action`, not `command`; `command` there is the Cumulocity fragment and stays), and state that the action is named by the topic only
- [ ] 11.2 Terminology: call the `system.toml` templates and the topic segment **actions**; keep `command` as the Cumulocity word only
- [ ] 11.3 `system.toml`: replace "requires a small schema extension" with the concrete decision — custom action templates are plain keys of `[init]`, and `deny_unknown_fields` is dropped, with the typo mitigations
- [ ] 11.4 Executor scoping: 0011 says the new subscription "covers only services of the agent's **own** device (`te/device/<own-device>/service/+/cmd/...`)" and then notes that this topic filter only works with the default topic scheme. Replace both with the actual design: the agent subscribes to the commands of every entity and decides per message, so the scope is no longer expressed by the subscription. The target is resolved through the entity store's REST API, except the device the agent runs on, which is recognized by comparing topic identifiers and needs no lookup
- [ ] 11.5 Exit codes: state them concretely — `0` success, `2` not supported for this service type, other non-zero failure
- [ ] 11.6 Future consideration "Workflow file naming collision": remove it. The operation name comes from the parsed `operation` field, never from the file name (`persist.rs:132`), so a file may be named anything
- [ ] 11.7 Capability clearing: state that withdrawal is implemented for command metadata on service entities, and that the rest of issue #2739 is out of scope. Add what stays: `c8y_ServiceCommand` remains a registered supported operation of the service even when the array becomes empty, so Cumulocity still offers the operation with no action to pick
- [ ] 11.8 Future consideration "Filter the capability to declare to cloud": keep it, and add the concrete case — a service declaring both `cmd/restart` and `cmd/collect_measurements` gets both in `c8y_SupportedServiceCommands` with no way to expose only one
- [ ] 11.9 Action names: state the rule `[a-z][a-z0-9_]+` where the `cmd/<action>` topic segment is described, and that a name with spaces or mixed case is out of scope even though MQTT topics allow it
- [ ] 11.10 Workflow `type` values: where 0011 gives `type = "service" # Can take one of the @type values: <device|child-device|service>`, add that the device an agent runs on is matched as `device` whatever `@type` the entity store reports for it, so `child-device` matches nothing today and is reserved for the future case of an agent driving another device's workflows
- [ ] 11.11 "Nothing is looked up across devices" is no longer true as written: an agent on a child device queries the main device's entity store over HTTP to learn the parent of a service. Reword it as being about execution — no agent executes a command on behalf of another device
- [ ] 11.12 Why-this-shape: "The workflow engine has no per-entity filtering, so once the agent subscribes, it drives the state machine for *every* service of its device" no longer holds — the agent now filters per message. The sole-driver contract stands, but its reason has to change: the agent deliberately claims every service of its own device, rather than being unable to filter
- [ ] 11.13 Cumulocity mapping: state that on a service entity **every** `cmd/<action>` capability becomes a service action, so the built-in per-operation mappings (`c8y_Restart`, `c8y_LogfileRequest`, `c8y_UploadConfigFile`, ...) no longer apply to a service. A service declaring `cmd/restart` gets `RESTART` in `c8y_SupportedServiceCommands`, not `c8y_Restart`. Nothing changes for a device
- [ ] 11.14 Cumulocity mapping: state that a capability whose action name breaks the rule of 11.9 is not declared to the cloud at all, only logged. Cumulocity would show a command whose lowercased name names no capability topic, so it could never be routed back
