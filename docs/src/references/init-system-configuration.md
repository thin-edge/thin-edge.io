---
title: Init System Configuration
tags: [Reference, Unix, Init, Services]
sidebar_position: 6
description: Configuring %%te%% to work with Linux init systems
---

To run %%te%% on a non-Systemd device, the file `/etc/tedge/system.toml` must be configured for the device init system.

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

Every key except `name` is an **action** of this init system,
and can be run with
[`tedge service <action> <service-name>`](./cli/tedge-service.md).

## Custom actions

An init system may support more than the five standard actions
`restart`, `stop`, `start`, `enable` and `disable`.
A custom action is added as a plain key of `[init]`,
with the same form as the others: an argv list with a `{}` placeholder for the service name.

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

# A custom action
reload = ["/bin/systemctl", "reload", "{}"]
```

`reload` can then be run as any other action:

```sh
sudo tedge service reload nginx
```

An action name is a single lowercase token, matching `[a-z][a-z0-9_-]*`:
lowercase letters, digits, `_` and `-`, starting with a letter.

:::caution
`[init]` accepts any key, so a misspelled key is read as a custom action rather than rejected.
Writing `restrat` instead of `restart` gives the device a `restrat` action it will never be asked for,
and leaves `restart` at its default.

Two things make such a typo visible:
the actions read from `[init]` are logged when the configuration is loaded,
and `tedge service` lists the actions it does know when it rejects one as unsupported.
:::

## Default settings

If the `system.toml` file does not exist, then %%te%% will assume that you are using Systemd, and use `/bin/systemctl` to control the services.
