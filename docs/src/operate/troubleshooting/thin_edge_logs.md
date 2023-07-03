---
title: Log Files
tags: [Operate, Log Files]
sidebar_position: 1
---

# The thin-edge logs
The logs that are useful for debugging thin-edge.io break down into logs that are created by thin-edge itself and by third party components.

## Thin-edge logs
On a thin-edge device different components like mappers, agent, and plugins run. The log messages of these components can be accessed as below.
The logs here capture INFO, WARNING, and ERROR messages.

### Cloud mapper logs
The thin-edge cloud mapper component that sends the measurement data to the cloud can be accessed as below.

#### Tedge Cumulocity mapper
The log messages of the Cumulocity mapper component that sends the measurement data from the thin-edge device to the Cumulocity
cloud can be accessed as below

```sh
journalctl -u tedge-mapper-c8y
```

:::note
Run `tedge-mapper --debug c8y` to log more debug messages
:::

#### Tedge Azure mapper
The log messages of the Azure mapper component that sends the measurement data from the thin-edge device to the Azure
cloud can be accessed as below.

```sh
journalctl -u tedge-mapper-az
```

:::note
Run `tedge-mapper --debug az` to log more debug messages
:::

#### Tedge AWS mapper
The log messages of the AWS mapper component that sends the measurement data from the thin-edge device to the AWS
cloud can be accessed as below.

```sh
journalctl -u tedge-mapper-aws
```

:::note
Run `tedge_mapper --debug aws` to log more debug messages
:::

### Device monitoring logs
The thin-edge device monitoring component logs can be found as below

#### Collectd mapper logs
The log messages of the collectd mapper that sends the monitoring data to the cloud can be accessed as below

```sh
journalctl -u tedge-mapper-collectd
```

:::note
Run `tedge-mapper --debug collectd` to log more debug messages
:::

### Software Management logs
This section describes how to access the software management component logs

#### Software update operation log
For every new software operation (list/update), a new log file will be created at `/var/log/tedge/agent`.
For each `plugin command` like prepare, update-list (install, remove), finalize, and list,
the log file captures `exit status, stdout, and stderr` messages.

#### Tedge Agent logs
The agent service logs can be accessed as below

```sh
journalctl -u tedge-agent
```

For example: tedge-agent logs plugin calls finalize and list.

```log title="Logs"
tedge-agent : TTY=unknown ; PWD=/tmp ; USER=root ; COMMAND=/etc/tedge/sm-plugins/apt finalize
tedge-agent : TTY=unknown ; PWD=/tmp ; USER=root ; COMMAND=/etc/tedge/sm-plugins/apt list
```

:::note
Run `tedge-agent --debug` to log more debug messages
:::

## Thirdparty component logs
Thin-edge uses the third-party components `Mosquitto` as the mqtt broker and `Collectd` for monitoring purpose.
The logs that are created by these components can be accessed on a thin-edge device as below.

### Mosquitto logs
Thin-edge uses `Mosquitto` as the `mqtt broker` for local communication as well as to communicate with the cloud.
The `Mosquitto` logs can be found in `/var/log/mosquitto/mosquitto.log`.
`Mosquitto` captures error, warning, notice, information, subscribe, and unsubscribe messages.

:::note
Set `log_type debug` or `log_type all` on `/etc/mosquitto/mosquitto.conf`, to capture more debug information.
:::

### Collectd logs
`Collectd` is used for monitoring the resource status of a thin-edge device.
Collectd logs all the messages at `/var/log/syslog`.
So, the collectd specific logs can be accessed using the `journalctl` as below

```sh
journalctl -u collectd
```

## Configuring log levels in thin-edge.io

The log levels can be configured for `thin-edge.io` services using either by command line or setting the required log
level in `system.toml`

### Setting the log level through cli

The log level can be enabled for a `thin-edge.io` service as below

For example for tedge-mapper:

```sh
sudo -u tedge -- tedge-mapper --debug c8y
```

:::note
In a similar way it can be set for all the `thin-edge.io` services.
Only `debug` level can be set through cli. Also, it enables `trace` level.
:::

### Setting log level through system.toml
The log levels can also be configured through the `system.toml` file.
The supported log levels are `info, warn, error, trace, debug`.

```toml title="file: /etc/tedge/system.toml"
[log]
tedge-mapper = "trace"
tedge-agent = "info"
tedge-watchdog = "debug"
c8y-log-plugin = "warn"
c8y-configuration-plugin = "error"
```

:::note
The log level strings are case insensitive
:::
