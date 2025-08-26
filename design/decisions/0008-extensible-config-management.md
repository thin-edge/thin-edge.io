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

  [docker.include]
  type_regex = "nginx"

  [docker.exclude]
  type_regex = "kube-*"
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

## Config management

### Phase 1

- Similar to log management, config plugins are defined under `/etc/tedge/config-plugins`.
- Config plugins can used to perform any pre/post processing steps before/after a configuration file is updated by the agent.
- Each plugin corresponds to their corresponding config types, unlike the log plugins where each plugin supports multiple types.
  There would be distinct plugins for each supported config type.
- The existing mechanism of defining the config file paths in the `tedge-configuration-plugin.toml` is retained with the agent.
  It won't be moved to a `file` plugin serving multiple config types as proposed for the log management,
  but each config file entry defined in the `toml` file can be supplemented with a plugin named after the same type.
- A config plugin needs to support the following sub-commands:
  - `info`: Used to detect if this is a valid plugin if it exits with exit code 0.
    The output must be the config type corresponding to this plugin.
  - `get <type> <tmp-file-path>`: Get the existing configuration for the given type.
    The config contents, if not already in a file, can be written to the `tmp-file-path` provided by the agent.
    Print the config file path to stdout between `:::begin-tedge:::` and `:::end-tedge:::` blocks.
    Either the original source config file path can be printed, or the `tmp-file-path` if the config contents were written to it.
  - `prepare <type>`: Perform any preparation steps like backing up existing config.
    Any relevant data to be passed to `rollback` or `commit` commands later (path of the backup file)
    must be printed to the console in json format:
    `{"meta": "<some-metadata>"}`
  - `set <type> [--url <new-config-url>] [--file <downloaded-file-path>]`: Update the existing config file
  - `validate <type>`: Validate if the applied configuration is successful
  - `rollback <type> --old-meta <json-output-from-prepare>`: Rollback the applied configuration if it failed in the `validate` phase.
  - `commit <type> --old-meta <json-output-from-prepare>`: Perform any post-update steps if the `validate` phase succeeded like deleting the backed up configs.
- File-based config types can still be defined in the existing `tedge-config-plugin.toml` file.
  But the pre/post processing logic of that configuration would be performed by the corresponding plugin for that type,
  if one is defined.
  If a plugin is not defined for the same type,
  the existing behavior of just updating the file on the fie system is maintained,
  without any `prepare`, `validate`, `commit` or `rollback` phases.
- For non file based configurations, that can't be defined in the `tedge-configuration-plugin.toml`,
  the plugin itself must declare its supported type with the `info` command.
- The `config-plugins` also has a `conf.d` sub-directory where extensions of the main config file can be created dynamically.
  Each extension config can have entries as follows:
  ```
  [[files]]
  type = "collectd"
  path = "/etc/collectd/collectd.conf"

  [[files]]
  type = "nginx"
  path = "/etc/nginx/nginx.conf"
  ```
- The supported config types are gathered from the main plugin config, its extension files
  and the names of all the plugins defined under `/etc/tedge/config-plugins`.

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
