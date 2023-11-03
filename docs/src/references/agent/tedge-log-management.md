---
title: Log File Management
tags: [Reference, Log Files]
sidebar_position: 7
---

# Log file management plugin

Thin-edge provides an operation plugin to fetch log files from the device.

* The log file management feature is provided with a `tedge-log-plugin` which runs as a daemon on thin-edge.
* The device owner can define the list of log files in the plugin's configuration file named `tedge-log-plugin.toml`.
* Each entry in the `tedge-log-plugin.toml` file contains a log `type` and a `path` pattern,
  where the `type` is used to represent the logical group of log files matching the `path` pattern.
* Upon receiving a log file upload command for a given `type`, 
  the log files for that type are retrieved using the `path` pattern defined in this `tedge-log-plugin.toml`,
  matched against the requested time range, search text and maximum line count.
* The plugin uploads the requested log file to the tedge file transfer repository.
  Its url is given by the received log upload command.
* The list of managed log files in `tedge-log-plugin.toml` can be updated both locally as well as from clouds, for instance, by using the configuration management feature.
* However, the plugin provides no direct connection to clouds, which is the responsibility of another component, i.e. the cloud mapper.
* The plugin has a dependency on the `tedge.toml` configuration file to get the MQTT hostname, port, and device identifier.
* The plugin establishes an MQTT connection to the broker using the `mqtt.bind.address` and `mqtt.bind.port` values from the `tedge.toml` configuration.
* The `<root>` and `<identifier>` for the topic to publish and subscribe are defined in `tedge.toml` file as `root.topic` and `device.topic`.

## Installation

As part of this plugin installation:
* On systemd-enabled devices, the service definition file for this `tedge-log-plugin` daemon is also installed.

Once installed, the `tedge-log-plugin` runs as a daemon on the device, listening to log update commands on the [`<root>/<identifier>/cmd/log_upload/+` MQTT topic](../mqtt-api.md).

## Configuration

The `tedge-log-plugin` configuration is stored by default under `/etc/tedge/plugins/tedge-log-plugin.toml`. If it does not exist on startup, the plugin creates the file with example contents.

This [TOML](https://toml.io/en/) file defines the list of log files that should be managed by the plugin.
The paths to these files can be represented using [glob](https://en.wikipedia.org/wiki/Glob_(programming)) patterns.
The `type` given to these paths are used as the log type associated to a log path.

```toml title="file: /etc/tedge/plugins/tedge-log-plugin.toml"
files = [
  { type = "mosquitto", path = '/var/log/mosquitto/mosquitto.log' },
  { type = "software-management", path = '/var/log/tedge/agent/software-*' },
  { type = "c8y_CustomOperation", path = '/var/log/tedge/agent/c8y_CustomOperation/*' }
]
```

The `tedge-log-plugin` parses this configuration file on startup for all the `type` values specified,
and sends the supported log types message to the MQTT local broker on the `<root>/<identifier>/cmd/log_upload` topic with a retained flag.

Given that `root.topic` and `device.topic` are set to `te` and `device/main//` for the main device,
the message to declare the supported log types is as follows.

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/log_upload' '{
  "types" : [ "mosquitto", "software-management", "c8y_CustomOperation" ]
}'
```

The plugin continuously watches this configuration file for any changes and resends the JSON message with the `type`s in this file,
whenever it is updated.

:::note
If the file `/etc/tedge/plugins/tedge-log-plugin.toml` is ill-formed or cannot be read,
then a JSON message with an empty array for the `types` field is sent, indicating no log files are tracked.
:::

## Handling log upload commands

The plugin subscribes to log upload commands on the [`<root>/<identifier>/cmd/log_upload/+` MQTT topic](../mqtt-api.md).
For example, it subscribes to the following topic for the main device.

```sh te2mqtt formats=v1
tedge mqtt sub 'te/device/main///cmd/log_upload/+'
```

A new log file upload command with the ID "1234" is published for the device named "example" by another component as below.

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/log_upload/1234' '{
  "status": "init",
  "tedgeUrl": "http://127.0.0.1:8000/tedge/file-transfer/example/log_upload/mosquitto-1234",
  "type": "mosquitto",
  "dateFrom": "2013-06-22T17:03:14.000+02:00",
  "dateTo": "2013-06-23T18:03:14.000+02:00",
  "searchText": "ERROR",
  "lines": 1000
}'
```

The plugin then checks the `tedge-log-plugin.toml` file for the log `type` in the incoming message (`mosquitto`),
retrieves the log files using the `path` glob pattern provided in the plugin config file,
including only the ones modified within the date range(`2013-06-22T17:03:14.000+02:00` to `2013-06-23T18:03:14.000+02:00`),
with the content filtered by the search text(`ERROR`) and the maximum line count(`1000`).

This filtered content is then uploaded to the URL received in the command as `tedgeUrl` via an HTTP PUT request.

During the process, the plugin updates the command status via MQTT
by publishing a retained message to the same `<root>/<identifier>/cmd/log_upload/<id>` topic,
where the command is received.

On the reception of a new log file upload command, the plugin updates the status to `executing`.
After successfully uploading the file to the file transfer repository, the plugin updates the status to `successful`.
If any unexpected error occurs, the plugin updates the status to `failed` with a `reason`.

Thus, the operation status update message for the above example looks like below.

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/log_upload/1234' '{
  "status": "failed",
  "reason": "The target log file for 'mosquitto' does not exist.",
  "tedgeUrl": "http://127.0.0.1:8000/tedge/file-transfer/example/log_upload/mosquitto-1234",
  "type": "mosquitto",
  "dateFrom": "2013-06-22T17:03:14.000+02:00",
  "dateTo": "2013-06-22T18:03:14.000+02:00",
  "searchText": "ERROR",
  "lines": 1000
}'
```

### Flow

```mermaid
sequenceDiagram
  participant Mapper/others
  participant Plugin
  participant Tedge Agent

  Mapper/others->>Plugin: tedge log_upload command (Status: init)
  Plugin->>Mapper/others: Status: executing
  alt No error
    Plugin->>Plugin: Extract log
    Plugin->>Tedge Agent: File upload [HTTP]
    Tedge Agent-->>Plugin: Status OK [HTTP]
    Plugin->>Mapper/others: Status: successful
  else Any error occurs
    Plugin->>Mapper/others: Status: failed
  end
```

## Usage

```sh
tedge-log-plugin --help
```

```run command="tedge-log-plugin --help" lang="text" title="Output"
Thin-edge device log file retriever

USAGE:
    tedge-log-plugin [OPTIONS]

OPTIONS:
        --config-dir <CONFIG_DIR>
            [default: /etc/tedge]

        --debug
            Turn-on the debug log level.

            If off only reports ERROR, WARN, and INFO If on also reports DEBUG

    -h, --help
            Print help information

    -V, --version
            Print version information

The thin-edge `CONFIG_DIR` is used:
  * to find the `tedge.toml` where the following configs are defined:
     ** `mqtt.bind.address` and `mqtt.bind.port`: to connect to the tedge MQTT broker
     ** `root.topic` and `device.topic`: for the MQTT topics to publish to and subscribe from
  * to find/store the `tedge-log-plugin.toml`: the configuration file that specifies which logs to be retrieved
```

## Logging

The `tedge-log-plugin` reports progress and errors to the OS journal which can be retrieved using `journalctl`.


