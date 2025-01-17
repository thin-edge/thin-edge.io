---
title: Configuration
tags: [Operate, Configuration]
sidebar_position: 2
description: How to configure %%te%%
---

The settings of all the %%te%% components are grouped in the `/etc/tedge/tedge.toml` file, using [TOML](https://toml.io/).
These configuration settings are organized in a hierarchy that reflects the component hierarchy.
For instance, all the settings related to Cumulocity share a `c8y` prefix, such as `c8y.url` for the Cumulocity URL.

This file can be edited directly and can even be extended to include plugin-specific settings.
However, it's recommended to use the [`tedge config`](../../references/cli/tedge-config.md) command
to edit the settings as it provides guidance for expected settings and checks for invalid entries.

## Common Commands

The following is a list of common commands which can be used to get/set/list %%te%% configuration.

### List configuration with descriptions

Display a complete list of available settings with their purpose.

```sh
tedge config list --doc
```

### List configuration that have been set or have defaults

List the settings for which a specific value has been set.

```sh
tedge config list
```

### Get a single configuration value

Display the value for the `c8y.url` setting, if one has been set.

```sh
tedge config get c8y.url
```

### Set configuration value

Update/set the value for the `c8y.url` setting.

```
tedge config set c8y.url mytenant.cumulocity.com`
```

### Reset a configuration value to use default value

Unset any user-specific value for the `c8y.url` setting, using then the default value.

```sh
tedge config unset c8y.url
```

## Examples

### Change path used for temporary files

The following shows how to change the temp directory used by %%te%% and its components.

1. Create a new directory which will be used by %%te%%

    ```sh
    # create a directory (with/without sudo)
    mkdir ~/tedge_tmp_dir

    # give ownership to tedge user and group
    sudo chown tedge:tedge ~/tedge_tmp_dir 
    ```

2. Update the tedge configuration to point to the newly created directory

    ```sh title="Example"
    sudo tedge config set tmp.path ~/tedge_tmp_dir
    ```

   :::info
   The directory must be available to `tedge` user and `tedge` group.
   :::

3. Restart the `tedge` daemons after any of these paths are updated, for it to take effect.

    ```sh
    sudo systemctl restart tedge-agent
    ```

To revert any of these paths back to their default locations, `unset` that config as follows:

```sh
sudo tedge config unset tmp.path
```

Then restart the relevant services.

```sh
sudo systemctl restart tedge-agent
```

## Customizing Settings

### Configuration Path

`/etc/tedge/tedge.toml` is the default location for %%te%% configuration.

This can be changed either by setting the `TEDGE_CONFIG_DIR` environment variable, or by passing an explicit `--config-dir` to all the %%te%% command invocations.

For instance, the following uses the `TEDGE_CONFIG_DIR` environment variable to change the default configuration directory so that all binaries will the new path.

```sh
export TEDGE_CONFIG_DIR=/tmp
tedge config set c8y.url mytenant.cumulocity.com
tedge-mapper c8y

# Revert back to the default config dir for once-off commands (e.g. /etc/tedge)
TEDGE_CONFIG_DIR= tedge-mapper c8y

# Or unset the environment variable
unset TEDGE_CONFIG_DIR
```

Alternatively, the following sample shows how to use the `--config-dir` flag to change the configuration directory, where the `/tmp/tedge.toml` configuration file will be used to set the `c8y.url` and launch the Cumulocity mapper.

```sh
tedge --config-dir /tmp config set c8y.url mytenant.cumulocity.com
tedge-mapper --config-dir /tmp c8y
```

### Environment variables

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

You can also use `tedge config` to inspect the value that is set, which may prove useful if you are using a mix of toml configuration and environment variables. For example, if you have already set the `c8y.url`, then you can read the value using:

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

### User-specific Configurations

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
