---
title: Configuration Overriding
tags: [Operate, Configuration]
description: Customizing %%te%% settings
---

## Configuration Path

`/etc/tedge/tedge.toml` is the default location for %%te%% configuration.

This can be changed by passing an explicit `--config-dir` to all the %%te%% command invocations.

For instance, the following uses `/tmp/tedge.toml` to set the `c8y.url` and launch the Cumulocity mapper.

```sh
tedge --config-dir /tmp config set c8y.url mytenant.cumulocity.com
tedge-mapper --config-dir /tmp c8y
```

## Environment variables

To aid in configuring %%te%% in containerised environments, %%te%% supports passing in the configuration via environment variables. For instance, to configure the Cumulocity URL and MQTT bind address, you can run:

```sh
env TEDGE_C8Y_URL=mytenant.cumulocity.com TEDGE_MQTT_BIND_ADDRESS=127.0.0.1 tedge connect c8y 
```

The environment variables won't be stored anywhere, so you will need to set the relevant values when running the mapper and agent:

```sh
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

```sh
tedge config get c8y.url
```

```text title="Output"
mytenant.cumulocity.com
```

Now we can run the same command but set an environment variable to override the value stored in the `tedge.toml` file.

```sh
env TEDGE_C8Y_URL=example.com tedge config get
```

```text title="Output"
example.com
```

## User-specific Configurations

The `/etc/tedge/tedge.toml` file can include extra settings used by user-specific plugins.

When the %%te%% commands (`tedge`, `tedge-agent`, `tedge-mapper`) detect a configuration setting they don't recognise,
they will emit a warning log message:

```sh
env TEDGE_C8Y_UNKNOWN_CONFIGURATION=test tedge config get c8y.url
```

```log title="Output"
2023-03-22 WARN tedge_config: Unknown configuration field "c8y.unknown_configuration" from environment variable TEDGE_C8Y_UNKNOWN_CONFIGURATION
mytenant.cumulocity.com
```
