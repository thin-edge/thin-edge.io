## ADDED Requirements

### Requirement: The tedge service command acts on a named service

The `tedge` CLI SHALL provide a `service` subcommand:

```
tedge service <action> <service-name> [--service-type <type>]
```

`<action>` names the action to perform, for example `start`, `stop` or `restart`.
It is a single lowercase token, matching `[a-z][a-z0-9_]+`,
the same rule that applies to the `cmd/<action>` topic segment.
`<service-name>` is the name of the service to act on.
`--service-type` selects the execution backend and SHALL default to `service` when omitted.

The command SHALL exit `0` on success,
and SHALL exit non-zero on failure with the reason written to stderr.
The exit code SHALL distinguish "the action failed" from
"the action is not supported for this service type",
so that a caller can tell a real failure from a missing backend.

Acting on a service usually requires root,
so `tedge service` is expected to be invoked under `sudo -n`,
which the sudoers rule shipped for `/usr/bin/tedge` already authorizes.
No new privileged surface is added.

#### Scenario: A service is restarted with the default type
- **WHEN** `tedge service restart collectd` is run
- **THEN** it SHALL act on `collectd` using the `service` backend and SHALL exit `0` on success

#### Scenario: A failing backend is reported
- **WHEN** the selected backend fails to run the action
- **THEN** `tedge service` SHALL exit non-zero and SHALL write the reason to stderr

#### Scenario: An unsupported action is distinguishable
- **WHEN** the action is not supported for the given service type
- **THEN** `tedge service` SHALL exit with the dedicated "not supported" exit code
  and SHALL NOT report a generic failure

### Requirement: tedge service dispatches on the service type

`tedge service` SHALL select its execution backend from the service type:

- the default type `service` SHALL be handled by the built-in init-system abstraction
  configured in `/etc/tedge/system.toml`;
- any other type `<type>` SHALL be handled by the service plugin at
  `/usr/share/tedge/service-plugins/<type>`;
- when no backend can handle the request — no action template for the `service` type,
  or no plugin file for the given type — the command SHALL fail
  with the "not supported" exit code and an error message naming the action and the service type.

The set of supported actions SHALL be decided by the backend, not by the CLI.
The CLI SHALL pass the action through unchanged.

#### Scenario: Default type uses the init system
- **WHEN** `tedge service restart collectd --service-type service` is run
  and `system.toml` defines the init system as systemd
- **THEN** it SHALL run the configured restart command for `collectd`, for example `systemctl restart collectd`

#### Scenario: A custom type uses its service plugin
- **WHEN** `tedge service restart nodered --service-type container` is run
- **THEN** it SHALL run `/usr/share/tedge/service-plugins/container restart nodered`

#### Scenario: No plugin installed for the type
- **WHEN** `tedge service restart nodered --service-type container` is run
  and `/usr/share/tedge/service-plugins/container` does not exist
- **THEN** the command SHALL fail with the "not supported" exit code and an error message naming the type

#### Scenario: No action template for the default type
- **WHEN** an action with no matching template in `system.toml` is run against the `service` type
- **THEN** the command SHALL fail with the "not supported" exit code

### Requirement: system.toml accepts custom action templates

The templates in `system.toml` are called **actions**,
to keep them apart from the thin-edge commands carried on `cmd/<action>` topics.

The `[init]` table SHALL accept action templates beyond the ones it defines today,
written as plain keys of that table,
so that a custom action such as `reload` can be supported for init-managed services.
A custom action template SHALL have the same form as the existing ones:
an argv list with a `{}` placeholder for the service name.

`[init]` SHALL therefore stop rejecting unknown keys.
To keep a misspelled key discoverable,
the actions read from `[init]` SHALL be logged when the configuration is loaded,
and `tedge service` SHALL list the known actions when it rejects an unsupported one.

Keys of `[init]` that are not service actions — the init system name
and the templates that query state rather than act on a service —
SHALL remain reserved and SHALL NOT be dispatchable as actions.

#### Scenario: A custom action template is honoured
- **WHEN** `[init]` defines a `reload` template and `tedge service reload nginx` is run
- **THEN** the configured `reload` command SHALL be executed

#### Scenario: A custom action without a template is not supported
- **WHEN** `[init]` defines no `reload` template and `tedge service reload nginx` is run
- **THEN** the command SHALL fail with the "not supported" exit code,
  and the error SHALL list the actions `[init]` does define

#### Scenario: A reserved key is not an action
- **WHEN** `tedge service` is run with an action name matching a reserved `[init]` key
- **THEN** it SHALL fail with the "not supported" exit code and SHALL NOT execute that template

### Requirement: Service plugin contract

A service plugin SHALL be an executable file at `/usr/share/tedge/service-plugins/<service-type>`,
named after the service type it handles.

`tedge service` SHALL invoke it as `<plugin> <action> <service-name>`,
where `<action>` is `start`, `stop`, `restart` or an action the plugin defines itself.

The plugin SHALL report its outcome through its exit code:

- `0`: the action succeeded;
- `2`: the action is not supported by this plugin, reserved for this meaning;
- any other non-zero code: the action failed, with the reason on stderr.

`tedge service` SHALL propagate the plugin's outcome:
exit `0` on `0`, the "not supported" outcome on `2`, and a failure otherwise.

An action name is a single lowercase token, `[a-z][a-z0-9_]+`,
so a plugin SHALL name the actions it defines within that rule.
It receives the name exactly as the service declared it,
and SHALL NOT have to accept any other spelling of it.

#### Scenario: Plugin succeeds
- **WHEN** the plugin exits `0`
- **THEN** `tedge service` SHALL exit `0`

#### Scenario: Plugin reports an unsupported action
- **WHEN** the plugin exits `2`
- **THEN** `tedge service` SHALL report the action as not supported for that service type

#### Scenario: Plugin fails
- **WHEN** the plugin exits `1` and writes a reason to stderr
- **THEN** `tedge service` SHALL exit non-zero and SHALL surface the plugin's stderr as the failure reason

### Requirement: tedge service validates its inputs before executing

Since `tedge service` runs as root, it SHALL validate every argument before use,
and SHALL reject an invalid argument without invoking any backend.

- the action SHALL be of bounded length and SHALL match `[a-z][a-z0-9_]+`.
  The rule allows neither a space nor a leading `-`,
  so the action can be neither read as an option nor mistaken for a command line to run;
- the service name SHALL be non-empty, of bounded length,
  match `[A-Za-z0-9_.@-]+`, and not start with `-`,
  so that it cannot be read as an option by the init tool;
- the service type SHALL match `[a-z0-9_-]+`,
  since it selects a file under the plugin directory and must not allow path traversal.

All execution SHALL be argv-based.
No argument SHALL be interpolated into a shell command line.

#### Scenario: An action with a space or a shell metacharacter is rejected
- **WHEN** the action argument contains whitespace or a shell metacharacter
- **THEN** `tedge service` SHALL reject the request and SHALL NOT invoke any backend

#### Scenario: An option-like service name is rejected
- **WHEN** the service name starts with `-`
- **THEN** `tedge service` SHALL reject the request and SHALL NOT invoke any backend

#### Scenario: A service type containing a path separator is rejected
- **WHEN** the service type contains `/` or `..`
- **THEN** `tedge service` SHALL reject the request and SHALL NOT resolve a plugin path from it

### Requirement: The service plugin directory is root-owned

Packaging SHALL create `/usr/share/tedge/service-plugins/` owned by `root:root` with mode `755`.
The directory SHALL NOT be writable by the `tedge` user.

Since `tedge service` runs as root, a directory writable by the `tedge` user
would let that user run arbitrary code as root.
This is the same property the sudoers path restriction enforces for sm-plugins today.

#### Scenario: Directory is created by packaging
- **WHEN** a thin-edge package is installed
- **THEN** `/usr/share/tedge/service-plugins/` SHALL exist, owned by `root:root` with mode `755`
