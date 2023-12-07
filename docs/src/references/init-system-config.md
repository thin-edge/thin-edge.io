---
title: Init System Configuration
tags: [Reference, Unix, Init, Services]
sidebar_position: 3
---

# Init System Configuration File

To support multiple init systems and service managers, `tedge` requires the `/etc/tedge/system.toml` file.
The file contains configurations about the init system and the supported actions.

The format of the file is:

```toml title="file: /etc/tedge/system.toml"
[init]
name = "systemd"
is_available = ["/bin/systemctl", "--version"]
restart = ["/bin/systemctl", "restart", "{}"]
stop =  ["/bin/systemctl", "stop", "{}"]
start =  ["/bin/systemctl", "start", "{}"]
enable =  ["/bin/systemctl", "enable", "{}"]
disable =  ["/bin/systemctl", "disable", "{}"]
is_active = ["/bin/systemctl", "is-active", "{}"]
```

:::info
For security reasons, the `system.toml` file should not be writable by non-root users. The permissions on the file can be set using the following command:

```sh
sudo chmod 644 /etc/tedge/system.toml
```
:::

## Placeholder

`{}` will be replaced by a service name (`mosquitto`, `tedge-mapper-c8y`, `tedge-mapper-az`, `tedge-mapper-aws`, etc.).
For example,

```toml
restart = ["/bin/systemctl", "restart", "{}"]
```

will be interpreted as

```sh
/bin/systemctl restart mosquitto
```

## Keys

| Property       | Description                                                                                          |
|----------------|------------------------------------------------------------------------------------------------------|
| `name`         | An identifier of the init system. It is used in the output of `tedge connect` and `tedge disconnect` |
| `is_available` | The command to check if the init is available on your system                                         |
| `restart`      | The command to restart a service by the init system                                                  |
| `stop`         | The command to stop a service by the init system                                                     |
| `start`        | The command to start a service by the init system                                                    |
| `enable`       | The command to enable a service by the init system                                                   |
| `disable`      | The command to disable a service by the init system                                                  |
| `is_active`    | The command to check if the service is running by the init system                                    |

## Default settings

If the `system.toml` file does not exist, then thin-edge will assume that you are using Systemd, and use `/bin/systemctl` to control the services.
