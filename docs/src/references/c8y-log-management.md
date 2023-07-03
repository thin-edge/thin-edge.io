---
title: Log File Management
tags: [Reference, Log Files]
sidebar_position: 7
---

# Log file management from Cumulocity

Thin-edge provides an operation plugin to [fetch log files from the device on to Cumulocity](https://cumulocity.com/guides/users-guide/device-management/#logs).

* Log file management from Cumulocity is provided with a `c8y-log-plugin` which runs as a daemon on thin-edge.
* The device owner can define the list of log files that can be retrieved from Cumulocity,
  in the plugin's configuration file named `c8y-log-plugin.toml`.
* Each entry in the the `c8y-log-plugin.toml` file contains a log `type` and a `path` pattern,
  where the `type` is used to represent the logical group of log files matching the `path` pattern.
* On receipt of a log file request for a given `type`, 
  the log files for that type are retrieved using the `path` pattern defined in this `c8y-log-plugin.toml`,
  matched against the requested time range, search text and maximum line count.
* The list of managed log files in `c8y-log-plugin.toml` can be updated both locally as well as from Cumulocity cloud,
  using the configuration management feature of Cumulocity, combined with the `c8y-configuration-plugin` of thin-edge.

## Installation

As part of this plugin installation:
* On systemd enabled devices, the service definition file for this `c8y-log-plugin` daemon is also installed as part of this plugin installation.

Once installed, the `c8y-log-plugin` is run as a daemon on the device listening to log requests from Cumulocity on `c8y/s/us` MQTT topic.
On startup, the plugin declares to the Cumulocity mapper that it supports `c8y_LogfileRequest` operation
by creating an empty file at `/etc/tedge/operations/c8y/c8y_LogfileRequest`.
It also reports all the log file types that it manages, defined in the `c8y-log-plugin.toml`

## Configuration

The `c8y-log-plugin` configuration is stored by default under `/etc/tedge/c8y/c8y-log-plugin.toml`.

This [TOML](https://toml.io/en/) file defines the list of log files that can be retrieved from the cloud tenant.
The paths to these files can be represented using [glob](https://en.wikipedia.org/wiki/Glob_(programming)) patterns.
The `type` given to these paths are used as the log type when they are reported to Cumulocity.

```toml title="file: /etc/tedge/c8y/c8y-log-plugin.toml"
files = [
  { type = "mosquitto", path = '/var/log/mosquitto/mosquitto.log' },
  { type = "software-management", path = '/var/log/tedge/agent/software-*' },
  { type = "c8y_CustomOperation", path = '/var/log/tedge/agent/c8y_CustomOperation/*' }
]
```

The `c8y-log-plugin` parses this configuration file on startup for all the `type` values specified,
and sends the supported log types message(SmartREST `118`) to Cumulocity on `c8y/s/us` topic as follows:

```csv
118,mosquitto,software-management,c8y_LogRequest,c8y_CustomOperation
```

The plugin continuously watches this configuration file for any changes and resends the `118` message with the `type`s in this file,
whenever it is updated.

:::note
If the file `/etc/tedge/c8y/c8y-log-plugin.toml` is ill-formed or cannot be read,
then an empty `118` message is sent, indicating no log files are tracked.
:::

## Handling log requests from Cumulocity

This plugin subscribes to `c8y/s/ds` topic, listening for `c8y_LogfileRequest` messages (SmartREST `522`) from Cumulocity, like this one:

```csv
522,<device-id>,mosquitto,2013-06-22T17:03:14.000+02:00,2013-06-22T18:03:14.000+02:00,ERROR,1000
```

The plugin then checks the `c8y-log-plugin.toml` file for the log type in the incoming message (`mosquitto`),
retrieves the log files using the `target` glob pattern provided in the plugin config file,
including only the ones modified within the date range(`2013-06-22T17:03:14.000+02:00` to `2013-06-22T18:03:14.000+02:00`),
with the content filtered by the search text(`ERROR`) and the maximum line count(`1000`).
This filtered content is then uploaded to Cumulocity as an event with the log `type` as the event `type`.

During this process, Cumulocity is notified of the progress of the `c8y_LogfileRequest` operation
with SmartREST messages `501`(executing), `502`(failed) or `503`(successful) on `c8y/s/us` topic.

## Updating supported log files from Cumulocity

The supported log files list defined in the `c8y-log-plugin.toml` can be updated both locally as well as from Cumulocity cloud.
Updates from Cumulocity can be achieved simply by listing this log plugin's config file(`c8y-log-plugin.toml`) 
in the configuration file of the `c8y-configuration-plugin`.
This will enable the `c8y-log-plugin.toml` to be tracked and managed by the `c8y-configuration-plugin`.

## Usage

```sh
c8y-log-plugin --help
```

```run command="c8y-log-plugin --help" lang="text" title="Output"
Thin-edge device log file retriever for Cumulocity

USAGE:
    c8y-log-plugin [OPTIONS]

OPTIONS:
        --config-dir <CONFIG_DIR>
            [default: /etc/tedge]

        --debug
            Turn-on the debug log level.

            If off only reports ERROR, WARN, and INFO If on also reports DEBUG and TRACE

    -h, --help
            Print help information

    -i, --init
            Create supported operation files

    -V, --version
            Print version information

On start, `c8y-log-plugin` notifies the cloud tenant of the log types listed in the `CONFIG_FILE`,
sending this list with a `118` on `c8y/s/us`.
`c8y-log-plugin` subscribes then to `c8y/s/ds` listening for logfile operation requests (`522`)
notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).

The thin-edge `CONFIG_DIR` is used to store:
  * c8y-log-plugin.toml - the configuration file that specifies which logs to be retrieved
```

## Logging

The `c8y-log-plugin` reports progress and errors to the OS journal which can be retrieved using `journalctl`.

## Future enhancements

To support retrieval of logs that are not available as physical files on the file system,
but can retrieved using other tools like `journalctl`, `docker logs` etc,
the log entries in `c8y-log-plugin.toml` can be enhanced to support retrieval of logs using any `command` execution.

Here is how the config for such logs retrieved using commands would look like:

```toml title="file: /etc/tedge/c8y/c8y-log-plugin.toml"
files = [
  { type = "mosquitto", path = '/var/log/mosquitto/mosquitto.log' },
  { type = "tedge-agent", command = '/usr/bin/journalctl --unit=tedge-agent --since=$FROM --until=$TO | grep $FILTER_TEXT' },
  { type = "<container id>", command = '/docker/log/script --target=$TARGET --from=$FROM --to=$TO --filter-text=$TEXT --line-count=$COUNT' }
]
```

The placeholders like `$TYPE`, `$FROM`, `$TO` etc in the `command` will be replaced with corresponding values in the SmartREST message.
If the logs can't be retrieved with a direct native command, scripts that takes the same inputs can also be written.
