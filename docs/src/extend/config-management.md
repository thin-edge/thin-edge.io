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

A config plugin is an executable that implements a simple command-line interface with the following requirements:

* Implement three sub-commands:
  1. **`list`** - Returns all config types supported by the plugin
  2. **`get <type>`** - Retrieves the current configuration for the specified type
  3. **`set <type> <new-config-path>`** - Updates the configuration for the specified type from the provided config file path
* Must exit with code 0 for when successful.
* Should output all config types supported by the plugin, one per line, for the `list` command.
* Should output the configuration content to stdout for the `get` command.
* Should handle validation, backup, restoration as well as any required service restart in the `set` command.
* Should handle errors gracefully, emitting those to `stderr`, and exit with non-zero codes on failure.

The agent automatically:
* Discovers plugins at startup by running their `list` command to gather their supported config types.
  skipping the ones returning a non zero exit code.
* Publishes the supported config types to the device's `config_snapshot` and `config_update` command metadata topics
  (e.g: `te/device/main///cmd/config_snapshot` and `te/device/main///cmd/config_update`)
  with a plugin suffix in the format `<config-type>::<plugin-name>` (e.g., `lighttpd.conf::lighttpd`)
* Routes `config_snapshot` requests to the `get` command of the appropriate plugin based on the type suffix.
* Routes `config_update` requests to the `set` command of the appropriate plugin based on the type suffix.
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

* The `list` command outputs the main configuration file: `lighttpd.conf`
* The `get` command retrieves the "effective configuration":
  - Combined the main configuration with all config extensions
  - Applies the default values for those not provided explicitly
* The `set` command implements a safe update process:
  - Validates the new configuration before applying it
  - Creates a backup of the existing configuration
  - Applies the new configuration
  - Restarts the `lighttpd` service
  - Restores from backup if the reload fails
  - Cleans up the backup on success

Here is the complete plugin:

```sh
#!/bin/sh
set -eu

usage() {
    echo "Usage: $0 <command> [args...]"
    echo "Commands:"
    echo "  list                                    List all config types supported by this plugin"
    echo "  get <type>                              Print the config for the specified type to stdout"
    echo "  set <type> <new-config-path>            Update the config for the specified type from the new config path"
    exit 1
}

list_config_types() {
    echo lighttpd.conf
}

get_config() {
    config_type="$1"

    if [ "$config_type" != "lighttpd.conf" ]; then
        echo "Error: Unsupported config type '$config_type'" >&2
        exit 1
    fi

    # Print the effective configuration, aggregating all config extensions and applying defaults
    lighttpd -p -f /etc/lighttpd/lighttpd.conf
}

set_config() {
    config_type="$1"
    target_config="$2"
    config_path="/etc/lighttpd/lighttpd.conf"
    backup_path="${config_path}.backup"

    if [ "$config_type" != "lighttpd.conf" ]; then
        echo "Error: Unsupported config type '$config_type'" >&2
        exit 1
    fi

    # Verify the new configuration before applying it
    if ! lighttpd -t -f "$target_config" 2>&1; then
        echo "Error: Configuration validation failed" >&2
        exit 1
    fi

    # Backup existing configuration
    if [ -f "$config_path" ]; then
        cp "$config_path" "$backup_path"
    fi

    # Apply new configuration
    mv "$target_config" "$config_path"

    # Restart the service and check if it succeeded
    if ! systemctl restart lighttpd; then
        echo "Error: Failed to reload lighttpd service, restoring backup" >&2
        if [ -f "$backup_path" ]; then
            mv "$backup_path" "$config_path"
            systemctl reload lighttpd
        fi
        exit 1
    fi

    rm -f "$backup_path"
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
        set)
            if [ $# -lt 2 ]; then
                echo "Error: 'set' command requires <type> and <new-config-path> arguments" >&2
                exit 1
            fi
            set_config "$@"
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

Update the configuration (note: this will actually modify your system):

```sh
sudo /usr/share/tedge/config-plugins/lighttpd set lighttpd.conf /path/to/new-lighttpd.conf
```

## Best Practices for Config Plugins

When creating custom config plugins, follow these best practices:

1. Validate the new configuration before applying it.
2. Always create a backup before updating a configuration, and restore it if the update fails.
3. After updating a configuration, reload or restart the service as appropriate.

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
