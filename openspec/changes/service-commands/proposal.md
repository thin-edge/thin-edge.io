## Why

Cumulocity can show action buttons (start, stop, restart, and custom commands) on a service,
but thin-edge cannot act on a service today.
Service operation directories are not watched dynamically,
the required `c8y_SupportedServiceCommands` fragment must be set by hand,
and the agent's workflow engine ignores commands addressed to service entities.
This change makes service actions work end to end, in a cloud-agnostic way,
for both init-managed services and services owned by a third-party daemon (for example containers).

See `design/decisions/0011-service-commands.md` for the domain background and the design rationale.

## What Changes

- Add a cloud-agnostic thin-edge interface for service commands.
  A service declares each supported command as a capability on
  `te/device/<device>/service/<service>/cmd/<action>`,
  and receives commands on the same per-command topics.
  Standard commands (`start`, `stop`, `restart`) and arbitrary custom commands are supported.
- Make tedge-agent the single executor for commands addressed to services of its own device.
  The workflow engine gains a `type` field so that a service `restart` and a device `restart`
  are distinct workflows.
- Add a `tedge service <command> <name> --service-type <type>` CLI.
  It dispatches on the service type:
  the built-in init-system abstraction (`system.toml`) for the default `service` type,
  or a service plugin at `/usr/share/tedge/service-plugins/<type>` for other types.
- Add native c8y mapper support.
  The mapper aggregates the declared `cmd/<action>` capabilities into `c8y_SupportedServiceCommands`,
  registers `c8y_ServiceCommand` as a supported operation (SmartREST `114`),
  converts an incoming `c8y_ServiceCommand` operation into a thin-edge command,
  and reports status back with the existing SmartREST `501`-`506` mapping.
- Extend the `system.toml` schema to accept custom action templates as plain keys of `[init]`.
  Today unknown keys are rejected via `deny_unknown_fields`.

For services that use this feature, declaring the service type at registration becomes effectively required.
A service registered without a type is dispatched as the default `service` (init-managed) type.

## Capabilities

### New Capabilities

- `service-commands`: the cloud-agnostic interface for declaring and issuing service commands,
  and the rule that tedge-agent is the sole executor for its own device's services.
  Covers the per-command topic model, workflow scoping by entity type, and the self-targeting rules
  (agent self-restart; rejecting `stop` of the agent or a cloud mapper).
- `tedge-service-cli`: the `tedge service` command and the service-plugin contract.
  Covers init-system dispatch via `system.toml`, the plugin invocation and exit codes,
  and the security validation of the command, service name, and service type.
- `c8y-service-commands`: the Cumulocity mapping.
  Covers capability aggregation into `c8y_SupportedServiceCommands`,
  conversion of a `c8y_ServiceCommand` operation into a thin-edge command,
  and status reporting.

### Modified Capabilities

None. No existing spec's requirements change.

## Impact

- `crates/core/tedge_agent` — subscribe to the own device's service command topics;
  workflow `type` scoping; shipped service-command workflows, including the agent self-restart pattern.
- `crates/core/tedge` — new `tedge service` subcommand.
- `crates/extensions/c8y_mapper_ext` — `c8y_ServiceCommand` operation handling,
  supported-operation registration (SmartREST `114`), and `c8y_SupportedServiceCommands` inventory updates.
- `crates/core/tedge_api` — workflow definition `type` field; the service command model.
- `system.toml` schema — accept custom action templates in `[init]`.
- Packaging — `/usr/share/tedge/service-plugins/` created `root:root 755`.
  The existing `tedge` sudoers rule already authorizes execution, so no new privileged surface is added.
- Tests — `tests/RobotFramework/tests/cumulocity/service_command/`.
