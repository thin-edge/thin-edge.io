# Extensible configuration management

* Date: __2025-08-12__
* Status: __New__

## Background

The built-in configuration and log management features offered by `tedge-agent` has the following limitations:

- only file based configurations and logs can be retrieved.
- while updating a configuration file, there are no pre/post actions that can be performed.
  For e.g, a user would want to backup the existing configuration before updating the configuration,
  and restart a service after the configuration is updated.
- Though the tedge workflows feature allows overriding the existing behavior,
  when a built-in operation is overridden, you end up losing all existing functionality,
  like uploading/downloading the files from the cloud as well.

The goal is to provide an interface that allows users to extend only the specific aspect of the operation that they want,
like the pre-update or post-update actions, leaving the rest of the behavior unchanged.

## Solution proposal

- The agent supports plugins for log and config management.
- The supported types for the respective operation is gathered from all these plugins.
- The agent maintains a mapping of the types and their corresponding plugins.
- When an an operation is received for a type, it is delegated to the corresponding plugin.

### Log Management

- `tedge-agent` to add support for multiple log management plugins defined under `/etc/tedge/log-plugins`.
- Migrate the existing file-based log functionality into a dedicated `file` plugin which is available by default.
- To retrieve logs from other sources like systemd journal or docker, add respective plugins for these sources.
- Each plugin must support the following sub-commands:
  - `list`: list all the log types that it supports.
    Used to detect if this is a valid plugin if it exits with exit code 0.
    The output is used only when auto-discovery for log types is turned ON for this plugin.
  - `get <type> <temp-log-file-path> [--from <timestamp>] [--to <timestamp>] [--filter <filter-text>]`:
    Used to fetch the log for the given type within the provided log range.
    The log content must be written to the temporary file passed provided by the agent.
- The plugin directory would also have a `conf.d` directory where each plugin can store their configuration in a toml file.
  These config files are used to turn type discovery on/off and to define inclusion/exclusion list when type discovery is turned on:
  ```docker.toml
  [docker]
  auto_discover=true

  [[docker.include]]
  pattern = "nginx"

  [[docker.exclude]]
  pattern = "kube-*"
  ```
- The `auto_discover` and `include`/`exclude` configs are used by the `tedge-agent`
  to process the supported types listed by that plugin.
  Any plugin-specific settings can also be defined in these files, which are ignored by the agent,
  but can be used by the plugin.
- The main config file for a plugin must be named after the plugin itself (e,g: `docker.sh` -> `docker.toml`).
  Key configs like `auto_discover` can only be defined in this main file.
  The inclusion/exclusion list can be defined in extension files as well,
  but the table names with the plugin name prefix (e.g: `[docker.include]` and `[docker.exclude]`) must be used in those as well.
- When new software is installed, their corresponding `include/exclude` entries can be appended to the main plugin config itself,
  or created in an independent extension file.

The agent uses the plugins as follows:

- On startup, gather the supported types from all the plugins by running the `list` command on them.
  Those failing to execute the `list` command are not qualified as valid plugins and ignored.
- Whenever the `/etc/tedge/log-plugins` directory is updated (new plugin installed or existing one removed),
  agent refreshes the supported types.
  Simply touching this directory would also trigger a refresh.
- When the supported types are published over mqtt, their source plugin information is also appended to that type
  in the format <log_type>::<plugin_type>.
  For example, if a `journals` plugin lists two different log types: `mosquitto` and `tedge-agent`,
  both types would be reported as `mosquitto::journald` and `tedge-agent::journald`.
- When a `log_upload` request is received for a type, call the `get` command of the corresponding plugin for that type
  which can be derived from the `::` suffix of that type.
  The `type` passed to the `get` command would not include the plugin suffix.
  If there is no explicit suffix, it is delegated to the default file plugin.

## Config management

### Phase 1

- Similar to log management, config plugins are defined under `/etc/tedge/config-plugins`.
- Config plugins can be used to perform any pre/post processing steps before/after a configuration file is updated by the agent.
- A config plugin needs to support the following sub-commands:
  - `list`: List all the config types supported by this plugin.
    Used to detect if this is a valid plugin if it exits with exit code 0.
    The types must be printed between `:::begin-tedge:::` and `:::end-tedge:::` marker lines,
    with one type per line.
  - `get <type> <tmp-target-file-path>`: Get the existing configuration for the given type.
  - `set <type> [--url <new-config-url>] [--file <new-config-file-path>]`: Update the existing config file
  - `finalize <type>`: Take any post-processing actions after the new config is applied
- Existing file-based config management using `tedge-configuration-plugin.toml` is moved to a default `file` plugin.
- The `tedge-configuration-plugin.toml` entries are provided an optional `exec` field to run any commands
  after the config file update, as follows:
  ```
  [[files]]
  type = "collectd"
  path = "/etc/collectd/collectd.conf"
  exec = "systemctl restart collectd"

  [[files]]
  type = "nginx"
  path = "/etc/nginx/nginx.conf"
  exec = "systemctl reload nginx"
  ```
- The `file` plugin's is implemented as follows:
  - `list`: list all the config types listed in `tedge-configuration-plugin.toml`.
  - `get`: Copy the contents of the existing configuration to the temp target file argument passed to it.
  - `set`: Replace the existing config file with new config file in the argument.
  - `finalize`: Perform the command specified in the `exec` field.
    No-op when no `exec` command is provided.
- For non file based configurations, that can't be defined in the `tedge-configuration-plugin.toml`,
  they must have dedicated plugins that declare their types in the `list` command.

The agent uses the plugins as follows:

- On startup, gather the supported types from all the plugins by running the `list` command on them.
  Those failing to execute the `list` command are not qualified as valid plugins and ignored.
- Whenever the `/etc/tedge/config-plugins` directory is updated (new plugin installed or existing one removed),
  agent refreshes the supported types.
  Simply touching this directory would also trigger a refresh.
- When the supported types are published over mqtt, their source plugin information is also appended to that type
  in the format <config_type>::<plugin_type>.
  For example, if a `mosquitto` plugin lists two different config types: `mosquitto.conf` and `mosquitto.acl`,
  both types would be reported as `mosquitto.conf::mosquitto` and `mosquitto.acl::mosquitto`.
- When a `config_snapshot` request is received for a type, call the `get` command of the corresponding plugin for that type
  which can be derived from the `::` suffix of that type.
  The `type` passed to the `get` command would not include the plugin suffix.
  If there is no explicit suffix, it is delegated to the default file plugin.
- When a `config_update` request is received for a type, the following actions are performed in sequence:
  - Call `get` command of the corresponding plugin and cache the target temporary file as a backup.
  - Call `set` command of the corresponding plugin with the updated config path in the argument.
  - Call `finalize` command of the plugin and complete the operation if successful.
  - If `set` or `finalize` commands fail, call the `set` command again with the original config file backup as the target.
  - Call `finalize` again if the previous `set` attempt to restore teh original configuration succeeded and fail the operation.

### Phase 2

Instead of the `exeucting` phase of `config_update` workflow doing everything in that single step,
break it down into multiple smaller stages that corresponds to each plugin command/phase as well as follows:

```config_update.toml
operation = "config_update"

[init]
action = "proceed"
on_success = "scheduled"

[scheduled]
action = "proceed"
on_success = "executing"

[executing]
action = "builtin:config_update:executing"
on_success = "download"

[download]
action = "builtin:download"
on_success = "prepare"

[prepare]
action = "builtin:backup_file"
on_success = "apply"

[apply]
action = "builtin:set_file"
on_success = "validate"

[validate]
action = "builtin:config_update:validate"
on_success = "commit"
on_error = "rollback"

[commit]
action = "builtin:config_update:apply"
on_success = "successful"

[rollback]
action = "builtin:config_update:rollback"
on_success = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
```

This helps customers override a single stage like `download` to do it their way,
while still reusing the rest of the functionality as-is.

### Unresolved problems

1. When a new piece of software is installed on the device,
   which mostly would have come with its own configurations and log files,
   how to "notify tedge" so that it refreshes its
   - updated software list
   - supported config list
   - updated logs list
2. Even if a REST API or inotify API to refresh the agent is provided,
   who would trigger this when a software is installed by various installation methods like:
   - software packages by distros
   - containers spawned by docker, podman, kubernetes etc
   Do they all provide hook points to trigger additional actions when a new thing is installed?

Since the installation of a new software is most likely to add new configurations and log,
the agent may do this all-in-one refresh implicitly whenever a `software_update` is performed.
The external refresh APIs only need to be used when new software is added outside the purview of tedge,
either manually by a device user, or an external software management system on the same device.