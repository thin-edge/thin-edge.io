---
title: Init System Configuration
tags: [Reference, Unix]
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
enable =  ["/bin/systemctl", "enable", "{}"]
disable =  ["/bin/systemctl", "disable", "{}"]
is_active = ["/bin/systemctl", "is-active", "{}"]
```

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

|Property|Description|
|--------|-----------|
|`name`|An identifier of the init system. It is used in the output of `tedge connect` and `tedge disconnect`|
|`is_available`|The command to check if the init is available on your system|
|`restart`|The command to restart a service by the init system|
|`stop`|The command to stop a service by the init system|
|`enable`|The command to enable a service by the init system|
|`disable`|The command to disable a service by the init system|
|`is_active`|The command to check if the service is running by the init system|
