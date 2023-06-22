---
title: Configuration Tools
tags: [Operate, Configuration]
sidebar_position: 1
---

# How-to configure `thin-edge.io`

`thin-edge.io` can be configured in a few different ways:

1. The `tedge config` command ([reference here](../../references/cli/tedge-config.md))
2. The `tedge.toml` file
3. Environment variables

## `tedge config` command

To set a value in `tedge.toml` using the `tedge` CLI, you can run:

```command
tedge config set c8y.url mytenant.cumulocity.com
```

Which will set `c8y.url` (the Cumulocity tenant URL) to `mytenant.cumulocity.com` and write the result to [`/etc/tedge/tedge.toml`](#tedgetoml). To read the value, run:

```command
tedge config get c8y.url
```

This will output the value of `c8y.url`

## `tedge.toml`

`/etc/tedge/tedge.toml` is the file `tedge config` writes to when making a configuration change. As the name suggests, this should be in the [toml format](https://toml.io/). To specify the Cumulocity URL and MQTT bind address, you can write:

```toml
[c8y]
url = "mytenant.cumulocity.com"

[mqtt]
bind_address = "127.0.0.1"
```

to `/etc/tedge/tedge.toml`.

## Environment variables

To aid in configuring `thin-edge.io` in containerised environments, `thin-edge.io` supports passing in the configuration via environment variables. For instance, to configure the Cumulocity URL and MQTT bind address, you can run:

```command
env TEDGE_C8Y_URL=mytenant.cumulocity.com TEDGE_MQTT_BIND_ADDRESS=127.0.0.1 tedge connect c8y 
```

The environment variables won't be stored anywhere, so you will need to set the relevant values when running the mapper and agent:

```command
env TEDGE_C8Y_URL=mytenant.cumulocity.com tedge-mapper c8y 
env TEDGE_C8Y_URL=mytenant.cumulocity.com tedge-agent 
```

The names for these environment variables are prefixed with `TEDGE_` to avoid conflicts with other applications, and any `.`s in the variable names are replaced with `_`s. Some example mappings are shown below:

| Setting             | Environment variable      |
| ------------------- | ------------------------- |
| `c8y.url`           | `TEDGE_C8Y_URL`           |
| `device.key_path`   | `TEDGE_DEVICE_KEY_PATH`   |
| `device.cert_path`  | `TEDGE_DEVICE_CERT_PATH`  |
| `mqtt.bind.address` | `TEDGE_MQTT_BIND_ADDRESS` |

You can also use `tedge config` to inspect the value that is set, which may prove useful if you are using a mix of toml configuration and environment variables. If you had tedge.toml file set as shown [above](#tedgetoml), you could run:

```command
tedge config get c8y.url
```

which outputs: *mytenant.cumulocity.com*

```command
env TEDGE_C8Y_URL=example.com tedge config get
```

which outputs: *example.com*

## Unrecognised configurations

When tedge commands (`tedge`, `tedge-agent`, `tedge-mapper`) detect a configuration setting they don't recognise, they will emit a warning log message[^1]:

```command
env TEDGE_C8Y_UNKNOWN_CONFIGURATION=test tedge config get c8y.url
```

```log
2023-03-22 WARN tedge_config: Unknown configuration field "c8y.unknown_configuration" from environment variable TEDGE_C8Y_UNKNOWN_CONFIGURATION
mytenant.cumulocity.com
```

[^1]: The log preamble has been abbreviated to aid readability here
