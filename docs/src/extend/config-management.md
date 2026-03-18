---
title: Configuration Management
tags: [Extend, Configuration Management]
sidebar_position: 3
description: Extend configuration management capabilities with plugins
---

This document describes how to extend the configuration management capabilities of %%te%% using plugins.

By default, %%te%% supports managing file-based configurations.
But, this out-of-the-box support is limited to reading or updating a file on the file system.

Custom configuration plugins enable more flexible configuration workflows:
1. Configuration files can be read from any source, such as the file system, a database, or a registry.
1. While fetching the configuration for a type, even if it is split across multiple files (e.g: config extensions),
   they can be combined into a single "effective configuration" before it is returned.
1. Pre-processing or post-processing steps can be performed while updating a configuration.
   Common pre-processing steps, include:
   - validating the new config
   - taking a backup of the existing config before the new config is applied
   Similarly, some commonly seen post-processing step are:
   - reloading/restarting the service using that config, for the updated config to take effect
   - restore the previously backed up config, if applying the new one fails.

## Overview

The %%te%% agent supports extensible configuration management through a plugin system:

* Plugins can be installed to support any kind of configuration sources, including non-file formats
* Each plugin can handle multiple configuration types
* Plugins are discovered and executed automatically by `tedge-agent`

## How Config Plugins Work

A config plugin is an executable that implements a command-line interface with the following sub-commands:

| Sub-command | Description |
|---|---|
| `list` | Outputs all config types supported by the plugin, one per line. Used to discover valid plugins — a non-zero exit code marks the executable as invalid. |
| `get <type>` | Outputs the current configuration for the specified type to **stdout**. |
| `prepare <type> <new-config-path> --work-dir <dir>` | Prepares for a config update: runs pre-checks (e.g. syntax validation) and creates a backup at `work-dir`. |
| `set <type> <new-config-path> --work-dir <dir>` | Applies the new configuration (e.g. moves the file into place, restarts the service). |
| `verify <type> --work-dir <dir>` | Verifies the configuration was applied successfully (e.g. checks the service is running). |
| `rollback <type> --work-dir <dir>` | Rolls back to the configuration that was active before the update, restoring the backup saved by the `prepare` step|

All sub-commands must exit with code `0` on success and write error messages to `stderr` with a non-zero exit code on failure.

The agent automatically:
* Discovers plugins at startup by running their `list` command to gather their supported config types,
  skipping any that return a non-zero exit code.
* Publishes the supported config types to the device's `config_snapshot` and `config_update` command metadata topics
  (e.g: `te/device/main///cmd/config_snapshot` and `te/device/main///cmd/config_update`)
  with a plugin suffix in the format `<config-type>::<plugin-name>` (e.g., `lighttpd.conf::lighttpd`).
* Routes `config_snapshot` requests to the `get` command of the appropriate plugin based on the type suffix.
* Routes `config_update` requests through the `prepare` → `set` → `verify` commands of the appropriate plugin,
  calling `rollback` automatically on any failure.
* The agent creates a temporary directory and passes that path as the `--work-dir` to `prepare` and `set`, `verify`, `rollback` steps and then deletes the same when the operation finishes (successfully or otherwise).
* Detects any new plugin installations dynamically.
* Refreshes the supported config types by reloading the plugins when any new software is installed or configuration is updated.

Config plugins are installed at `/usr/share/tedge/config-plugins`, by default.

This plugin root directory can be changed using:
```sh
sudo tedge config set configuration.plugin_paths /usr/local/share/tedge/config-plugins,/usr/share/tedge/config-plugins
```

Multiple plugin directories can be specified if a layered directory structure is desired,
where plugins from directories earlier in the list gets precedence over the latter ones.
For example, with the above configuration, a `file` plugin in `/usr/local/share/tedge/config-plugins`
would override the one with the same name in `/usr/share/tedge/config-plugins`.
This mechanism can be used to override the factory plugins when needed.

## Permissions

Plugins are executed by the agent with `sudo` privileges.
When %%te%% is installed using the official packages available for the supported package managers,
the `tedge` package automatically creates the following sudoers entry,
giving sudo rights to all plugins installed at `/usr/share/tedge/config-plugins`:

```
tedge    ALL = (ALL) NOPASSWD:SETENV: /usr/share/tedge/config-plugins/[a-zA-Z0-9]*
```

If %%te%% is installed using some alternative installation methods,
then the above sudoers entry must also be added explicitly. 

If the `config.plugin_paths` config is updated with additional directories as shown in the previous section,
then sudoers entries must be created for those directories as well.

Additionally, ensure your plugin has appropriate permissions to access the configuration sources it needs.

## Creating a Custom Config Plugin

### Example: lighttpd plugin

The following example demonstrates a complete plugin implementation for managing the lighttpd web server configuration, showcasing how to handle multi-file configurations, validation, backup/restore, and service reloading.

Key features of this plugin are:

* The `list` command outputs the main configuration file: `lighttpd.conf`.
* The `get` command retrieves the "effective configuration" by running `lighttpd -p`,
  which combines the main configuration with all extensions and applies defaults.
* The `prepare` command validates the new configuration with `lighttpd -t`,
  then saves a backup of the existing configuration to the `--work-dir`.
* The `set` command moves the new configuration into place and restarts `lighttpd`.
  It exits with a non-zero code if the service fails to restart,
  which causes the agent to call `rollback` automatically.
* The `verify` command confirms that `lighttpd` is running after the update.
* The `rollback` command restores the backup saved in `--work-dir` and restarts the service.

Here is the complete plugin:

```sh
#!/bin/sh
set -eu

usage() {
    echo "Usage: $0 <command> [args...]"
    echo "Commands:"
    echo "  list                                                 List all config types supported by this plugin"
    echo "  get <type>                                           Print the config for the specified type to stdout"
    echo "  prepare <type> <new-config-path> --work-dir <dir>   Prepare for configuration update (create backup)"
    echo "  set <type> <new-config-path> --work-dir <dir>       Update the config for the specified type and restart service"
    echo "  verify <type> --work-dir <dir>                       Verify configuration was applied successfully"
    echo "  rollback <type> --work-dir <dir>                     Rollback configuration to previous state"
    exit 1
}

list_config_types() {
    echo lighttpd.conf
}

get_config() {
    config_type="$1"

    validate_config_type "$config_type"

    # Print the effective configuration, aggregating all config extensions and applying defaults
    lighttpd -p -f /etc/lighttpd/lighttpd.conf
}

parse_work_dir() {
    work_dir=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --work-dir)
                if [ -z "${2:-}" ]; then
                    echo "Error: --work-dir requires a value" >&2
                    exit 1
                fi
                work_dir="$2"
                shift 2
                ;;
            *)
                echo "Error: Unknown argument '$1'" >&2
                exit 1
                ;;
        esac
    done

    if [ -z "$work_dir" ]; then
        echo "Error: --work-dir is required" >&2
        exit 1
    fi

    echo "$work_dir"
}

validate_config_type() {
    config_type="$1"
    if [ "$config_type" != "lighttpd.conf" ]; then
        echo "Error: Unsupported config type '$config_type'" >&2
        exit 1
    fi
}

prepare_config() {
    config_type="$1"
    new_config_path="$2"
    shift 2

    validate_config_type "$config_type"

    if [ ! -f "$new_config_path" ]; then
        echo "Error: New config file not found: $new_config_path" >&2
        exit 1
    fi

    # Parse --work-dir argument
    work_dir=$(parse_work_dir "$@")

    # Verify the new configuration before applying it
    if ! lighttpd -t -f "$new_config_path" 2>&1; then
        echo "Error: Configuration validation failed" >&2
        exit 1
    fi

    config_path="/etc/lighttpd/lighttpd.conf"
    backup_path="${work_dir}/lighttpd.conf.backup"

    # Backup existing configuration to work directory
    if [ -f "$config_path" ]; then
        cp "$config_path" "$backup_path"
    fi
}

set_config() {
    config_type="$1"
    target_config="$2"
    shift 2
    config_path="/etc/lighttpd/lighttpd.conf"

    validate_config_type "$config_type"

    # Parse --work-dir argument
    work_dir=$(parse_work_dir "$@")

    # Apply new configuration
    mv "$target_config" "$config_path"

    # Restart the service
    if ! systemctl restart lighttpd; then
        echo "Error: Failed to restart lighttpd service" >&2
        exit 1
    fi
}

verify_config() {
    config_type="$1"
    shift

    validate_config_type "$config_type"

    # Parse --work-dir argument
    work_dir=$(parse_work_dir "$@")

    # Verify the service is running
    if ! systemctl is-active --quiet lighttpd; then
        echo "Error: lighttpd service is not running" >&2
        exit 1
    fi
}

rollback_config() {
    config_type="$1"
    shift

    validate_config_type "$config_type"

    # Parse --work-dir argument
    work_dir=$(parse_work_dir "$@")

    config_path="/etc/lighttpd/lighttpd.conf"
    backup_path="${work_dir}/lighttpd.conf.backup"

    # Restore backup configuration
    if [ -f "$backup_path" ]; then
        mv "$backup_path" "$config_path"
        if ! systemctl restart lighttpd; then
            echo "Error: Failed to restart service after rollback" >&2
            exit 2
        fi
    else
        echo "Error: Backup file not found at $backup_path" >&2
        exit 1
    fi
}

main() {
    if [ $# -lt 1 ]; then
        usage
    fi

    command="$1"
    shift

    case "$command" in
        list)
            list_config_types
            ;;
        get)
            if [ $# -lt 1 ]; then
                echo "Error: 'get' command requires a <type> argument" >&2
                exit 1
            fi
            get_config "$@"
            ;;
        prepare)
            if [ $# -lt 1 ]; then
                echo "Error: 'prepare' command requires <type> and --work-dir arguments" >&2
                exit 1
            fi
            prepare_config "$@"
            ;;
        set)
            if [ $# -lt 2 ]; then
                echo "Error: 'set' command requires <type> and <new-config-path> arguments" >&2
                exit 1
            fi
            set_config "$@"
            ;;
        verify)
            if [ $# -lt 1 ]; then
                echo "Error: 'verify' command requires <type> and --work-dir arguments" >&2
                exit 1
            fi
            verify_config "$@"
            ;;
        rollback)
            if [ $# -lt 1 ]; then
                echo "Error: 'rollback' command requires <type> and --work-dir arguments" >&2
                exit 1
            fi
            rollback_config "$@"
            ;;
        *)
            echo "Error: Unknown command '$command'" >&2
            usage
            ;;
    esac
}

main "$@"
```

### Installation

Copy the plugin to the plugins directory and make it executable:

```sh
sudo cp /path/to/lighttpd /usr/share/tedge/config-plugins/lighttpd
sudo chmod +x /usr/share/tedge/config-plugins/lighttpd
```

### Testing the Plugin

List all config types the plugin supports:

```sh
sudo /usr/share/tedge/config-plugins/lighttpd list
```

Retrieve the current configuration:

```sh
sudo /usr/share/tedge/config-plugins/lighttpd get lighttpd.conf
```

Test each stage of a configuration update (note: `prepare` and `set` will modify your system):

```sh
WORK_DIR=$(mktemp -d)

# Prepare: validate and back up the existing config
sudo /usr/share/tedge/config-plugins/lighttpd prepare lighttpd.conf /path/to/new-lighttpd.conf --work-dir "$WORK_DIR"

# Set: move the new config into place and restart the service
sudo /usr/share/tedge/config-plugins/lighttpd set lighttpd.conf /path/to/new-lighttpd.conf --work-dir "$WORK_DIR"

# Verify: confirm the service is running with the new config
sudo /usr/share/tedge/config-plugins/lighttpd verify lighttpd.conf --work-dir "$WORK_DIR"

# Rollback: restore the backup (run this instead of set/verify if something goes wrong)
sudo /usr/share/tedge/config-plugins/lighttpd rollback lighttpd.conf --work-dir "$WORK_DIR"
```

## Best Practices for Config Plugins

1. `get`: Write the complete, effective configuration to stdout.
   For services that aggregate multiple config fragments, resolve them before output.
1. `prepare`: Validate the new configuration before applying it.
   Save any state needed for rollback (e.g. a backup of the existing config) to work-dir.
1. `set`: Apply the configuration and restart/reload any associated services.
1. `verify`: Check that the service is actually running correctly after the update,
    or whether it was really restarted, if a restart was requested in `set`.
1. `rollback`: Restore the backup you saved in `prepare` and restart the service.

## Refresh Supported Config Types

To ensure that any newly installed services or configuration sources are immediately available for configuration management,
`tedge-agent` automatically reloads the plugins and refreshes the supported config types on the following events:

* A new config plugin is installed in the plugin directory (`/usr/share/tedge/config-plugins`)
* The `tedge-config-plugin.toml` file is updated
* A new software is installed with the agent (via the `software_update` command)

A refresh can also be triggered manually by sending sync signals to the agent as follows:

```sh
tedge mqtt pub te/device/main/service/tedge-agent/signal/sync_config '{}'
```

The agent reacts to all these events by gathering the latest supported config types from all the installed plugins
by invoking the `list` command on them,
and publishes the aggregated types to the `te/device/main///cmd/config_snapshot` and `te/device/main///cmd/config_update` meta topics.

## Config Update Workflow

Configuration updates are driven by a workflow definition at `/etc/tedge/operations/config_update.toml`.
The agent creates this file with a default definition on first startup, which can be customized by the user.

The default workflow is:

```toml title="file: /etc/tedge/operations/config_update.toml"
operation = "config_update"
on_error = "failed"

[init]
action = "proceed"
on_success = "executing"

[executing]
action = "proceed"
on_success = "download"

[download]
action = "download"
on_success = "prepare"

[prepare]
action = "builtin:config_update:prepare"
input.setFrom = "${.payload.downloadedPath}"
on_success = "set"

[set]
action = "builtin:config_update:set"
on_success = "evaluate-agent-restart"
on_error = "rollback"

[evaluate-agent-restart]
script = "test ${.payload.restartAgent} = true"
on_exit.0 = "restart-agent"
on_exit.1 = "verify"

[restart-agent]
action = "restart-agent"
on_exec = "await-agent-restart"

[await-agent-restart]
action = "await-agent-restart"
timeout_second = 90
on_timeout = "rollback"
on_success = "verify"

[verify]
action = "builtin:config_update:verify"
on_success = "successful"
on_error = "rollback"

[rollback]
action = "builtin:config_update:rollback"
on_success = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
```

### Builtin actions

The workflow uses these built-in agent capabilities:

| Action | Description |
|---|---|
| `download` | Downloads the file from the URL in `tedgeUrl` or `remoteUrl` in the command payload. Stores it at a temporary path and adds `downloadedPath` to the operation payload. |
| `builtin:config_update:prepare` | Creates the `--work-dir` directory, then calls the plugin's `prepare` command. The `input.setFrom` field supplies the downloaded file as the `<new-config-path>` argument. |
| `builtin:config_update:set` | Calls the plugin's `set` command with the downloaded config path and `--work-dir`. |
| `builtin:config_update:verify` | Calls the plugin's `verify` command with `--work-dir`. |
| `builtin:config_update:rollback` | Calls the plugin's `rollback` command with `--work-dir`, then deletes the work directory. |

### Requesting an agent restart

If the configuration update requires restarting `tedge-agent` itself
(for example, when updating a %%te%% configuration file like `tedge.toml`),
set `"restartAgent": true` in the command payload:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/config_update/1234' '{
  "status": "init",
  "tedgeUrl": "http://127.0.0.1:8000/te/v1/files/config_update/tedge-config-1234",
  "type": "tedge.toml",
  "restartAgent": true
}'
```

The workflow evaluates this flag after `set` completes: if `true`,
the agent restarts itself and waits up to 90 seconds to resume.
The `verify` step then runs after the restart.
If the agent does not come back within 90 seconds, the `rollback` is performed.

:::note
When using the out-of-the-box configuration update, powered by the `file` plugin,
no rollback to the old configuration is attempted to maintain backward compatibility.
:::

### Customizing the workflow

Because the workflow is a standard TOML file at `/etc/tedge/operations/config_update.toml`,
you can override any stage while keeping the built-in steps for everything else.
See [Operation Workflows](../references/agent/operation-workflow.md) for the full workflow syntax.
