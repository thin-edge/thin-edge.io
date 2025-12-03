---
title: Log Management
tags: [Extend, Log Management]
sidebar_position: 2
description: Extend log management capabilities with plugins
---

This document describes how to extend the log management capabilities of %%te%% using custom log plugins.
By default, %%te%% supports retrieving file-based logs.
But, it can be extended with plugins to collect logs from other sources like systemd journal,
docker containers or any custom log sources.

## Overview

The %%te%% agent supports extensible log management through a plugin system:

* The built-in `file` plugin handles traditional file-based logs (the default behavior)
* Additional plugins can be installed to support other log sources
* Each plugin can provide multiple log types
* Plugins are discovered and executed automatically by `tedge-agent`

## How Log Plugins Work

A log plugin is an executable that implements a simple command-line interface with the following requirements:

* Implement two sub-commands:
  1. **`list`** - Returns all log types supported by the plugin (one per line)
  2. **`get <log-type> [--since <timestamp>] [--until <timestamp>]`** - Retrieves logs for the specified type within the given time range
* Must exit with code 0 for successful `list` command (used to validate the plugin).
* Should output logs to stdout for the `get` command.
* Time filters `--since` and `--until` are passed as seconds since epoch.
* Should handle errors gracefully and exit with non-zero codes on failure.

The agent automatically:
* Discovers plugins at startup by running their `list` command to gather their supported log types.
* Publishes the supported log types to the device's `log_upload` command metadata topic (e.g: `te/device/main///cmd/log_upload`)
  with a plugin suffix in the format `<log-type>::<plugin-name>` (e.g., `mosquitto::journald`)
* Routes `log_upload` requests to the `get` command of the appropriate plugin based on the type suffix.
* The `dateFrom` and `dateTo` parameters in the command are passed to the plugin as `--since` and `--until` arguments.
* Further filtering by `searchText` and tail `lines` are done by the agent itself.
* Detects any new plugin installations dynamically.
* Refresh the supported log types by reloading the plugins when any new software is installed or configuration is updated.

Log plugins are installed at `/usr/share/tedge/log-plugins`, by default.

This plugin root directory can be changed using:
```sh
sudo tedge config set log.plugin_paths /usr/local/share/tedge/log-plugins,/usr/share/tedge/log-plugins
```

Multiple plugin directories can be specified if a layered directory structure is desired,
where plugins from directories earlier in the list gets precedence over the latter ones.
For example, with the above configuration, a `file` plugin in `/usr/local/share/tedge/log-plugins`
would override the one with the same name in `/usr/share/tedge/log-plugins`.
This mechanism can be used to override the factory plugins when needed.

## Permissions

Plugins are executed by the agent with `sudo` privileges.
The agent automatically creates the following sudoers entry,
giving sudo rights to all plugins installed at `/usr/local/lib/tedge/log-plugins`:

```
tedge    ALL = (ALL) NOPASSWD:SETENV: /usr/local/lib/tedge/log-plugins/[a-zA-Z0-9]*
```

If the `log.plugin_paths` config is updated with additional directories as shown in the previous section,
then sudoers entries must be created for those directories as well.

Additionally, ensure your plugin has appropriate permissions to access the log sources it needs.

## Factory Plugins

* The default `file` plugin is included in the `tedge` installation package itself on all distributions.
* A `journald` plugin that can gather systemd service logs using the `journalctl` command is also included
  in the `tedge` packages for systemd based distributions like Debian, Ubuntu, RHEL etc.

## Filtering Plugin Log Types

When a plugin is listing too many log types that the user is not interested in,
the irrelevant entries can be filtered out by specifying `include` and `exclude` filter patterns
in the `tedge-log-plugin.toml` configuration file.

### Configuration Format

Use the `[[plugins.<plugin-name>.filters]]` sections to define the filters.
Both `include` and `exclude` support regex patterns.
Multiple `[[plugins.<plugin-name>.filters]]` sections can be defined for the same plugin
so that it is easier to extend by just appending new entries.

```toml title="file: /etc/tedge/plugins/tedge-log-plugin.toml"
[[plugins.journald.filters]]
include = "tedge-.*"

[[plugins.docker.filters]]
exclude = "kube-.*"
```

### Filtering Rules

- If no filters are defined for a plugin, all log types listed by that plugin are accepted.
- When only `include` patterns are defined, the log types matching any one `include` pattern are accepted.
- When only `exclude` patterns are defined, the log types that doesn't match any exclude pattern are accepted.
- When both `include` and `exclude` patterns are provided, the log types that doesn't match any exclude pattern
  and also those that match the `include` patterns are accepted.
  This means include patterns can selectively override types that would otherwise have been excluded.

### Examples

The `journald` plugin is used in all the subsequent examples.

#### Include only the tedge services

```toml
[[plugins.journald.filters]]
include = "tedge-*"
```

#### Exclude all services starting with systemd

```toml
[[plugins.journald.filters]]
exclude = "systemd-*"
```

#### Exclude all systemd services except systemd-logind

```toml
[[plugins.journald.filters]]
include = "systemd-logind"

[[plugins.journald.filters]]
exclude = "systemd-*"
```

### Reloading After Configuration Changes

Whenever the plugin filters in `tedge-log-plugin.toml` are modified,
the agent automatically reloads the plugins and applies the new filters.
You can verify the filtered log types by checking the published `log_upload` command metadata.
For example, on the main device, subscribe to:

```sh
tedge mqtt sub 'te/device/main///cmd/log_upload'
```

## Creating a Custom Log Plugin

### Example: docker plugin

Here's a `docker` plugin example that can retrieve logs from containers using the `docker logs` command:

```sh
#!/bin/sh
set -eu

help() {
    cat <<EOT
docker log plugin to retrieve the logs from containers using the docker cli

$0 <SUBCOMMAND>

SUBCOMMANDS
  list
  get <type> [--since <timestamp>] [--until <timestamp>]
EOT
}

list_log_types() {
    docker ps -a --format "{{.Names}}"
}

get_log_by_type() {
    log_type="$1"
    shift

    # Parse option defaults
    since="24h"
    until="0s"

    while [ $# -gt 0 ]; do
        case "$1" in
            --since)
                since="$2"
                shift
                ;;
            --until)
                until="$2"
                shift
                ;;
        esac
        shift
    done

    # Retrieve logs using docker logs
    docker logs "$log_type" \
        --since "$since" \
        --until "$until" \
        2>&1
}

if [ $# -lt 1 ]; then
    echo "Missing required subcommand" >&2
    help
    exit 1
fi

SUBCOMMAND="$1"
shift

case "$SUBCOMMAND" in
    list)
        list_log_types
        ;;
    get)
        get_log_by_type "$@"
        ;;
    *)
        echo "Unsupported command" >&2
        exit 1
        ;;
esac
```

* The `list` command of this plugin will output all container names:
  ```
  nginx
  mosquitto
  ```

* The `get` command retrieves the logs for the target container using the `docker logs` command.

### Installation

Copy the plugin to the plugins directory and make it executable:

```sh
sudo cp /path/to/docker /usr/share/tedge/log-plugins/
sudo chmod +x /usr/share/tedge/log-plugins/docker
```

### Testing the Plugin

List all log types the plugin supports:

```sh
sudo /usr/share/tedge/log-plugins/docker list
```

Retrieve logs for a specific service:

```sh
sudo /usr/share/tedge/log-plugins/docker get ssh
```

Retrieve logs with time filters (timestamps in seconds since epoch):

```sh
sudo /usr/share/tedge/log-plugins/docker get tedge-agent --since 1696250000 --until 1696260000
```

## Refresh Supported Log Types

To ensure that any newly installed services or log sources are immediately available for log collection,
`tedge-agent` automatically reloads the plugins and refreshes the supported log types on the following events:

* A new log plugin is installed in the plugin directory (`/usr/share/tedge/log-plugins`)
* The `tedge-log-plugin.toml` file is updated
* A new software is installed with the agent (via the `software_update` command)
* A new configuration is installed or updated with the agent (via the `config_update` command)

A refresh can also be triggered manually by sending a sync signal to the agent as follows:

```
tedge mqtt pub te/device/main/service/tedge-agent/signal/sync_log_upload '{}'
```

The agent reacts to all these events by gathering the latest supported log types from all the installed plugins
by invoking the `list` command on them,
and publishes the aggregated types to the `te/device/main///cmd/log_upload` meta topic.
