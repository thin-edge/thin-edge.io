---
title: Service Plugin API
tags: [Reference, API, Services]
sidebar_position: 11
description: Service Plugin API reference
---

# Service Plugin API

A **service plugin** runs the actions of the services that the init system does not manage:
containers, an application-specific supervisor, or anything else with its own way to start and stop things.

[`tedge service`](./cli/tedge-service.md) is the runner.
It dispatches on the service type:
the default type `service` is handled by the init system configured in
[`system.toml`](./init-system-configuration.md),
and any other type by the plugin named after it.

## Runner

The runner is the `tedge service` command,
called either by an operator or by the
[service command workflows](./agent/service-commands.md) of `tedge-agent`.

### Backends

The service type selects the backend that runs the action.

| Service type          | Backend                                                                            |
|-----------------------|--------------------------------------------------------------------------------------|
| `service` (default)   | The init system, as configured in [`system.toml`](./init-system-configuration.md)  |
| any other type `<t>`  | The service plugin `<plugin-dir>/<t>`                                              |

The plugin directories are the `tedge config` value `service.plugin_paths`,
a comma-separated list, `/usr/share/tedge/service-plugins` per default.
A plugin is picked by its exact name, the service type,
from the first directory that holds one:

```sh
sudo tedge config set service.plugin_paths /usr/local/share/tedge/service-plugins,/usr/share/tedge/service-plugins
```

```sh
# Handled by the init system, e.g. systemctl restart collectd
sudo tedge service restart collectd

# Handled by /usr/share/tedge/service-plugins/container
sudo tedge service restart nodered --service-type container
```

Acting on a service usually requires root, so the runner is normally called under `sudo`.
The sudoers rule shipped for `/usr/bin/tedge` already authorizes it,
so no privileged surface is added.

The set of supported actions is decided by the backend, not by the runner.
The action name is passed through unchanged.
For the default service type, the actions are the templates defined in `system.toml`.
For any other type, they are whatever the plugin implements.

### Running a plugin

* For `tedge service <action> <name> --service-type <type>`,
  the runner executes `<plugin-dir>/<type> <action> <name>`.
* The runner forwards the plugin's stdout and stderr to its own,
  and uses the last non-empty line of stderr as the reason of a failure.
* The runner maps the plugin's exit code to its own:
  `0` stays `0`, `2` stays `2`, any other non-zero code becomes a failure with that reason.
* A service type with no plugin file is reported as not supported, with exit code `2`.

### Exit codes

| Code           | Meaning                                                                     |
|----------------|-------------------------------------------------------------------------------|
| `0`            | The action succeeded                                                        |
| `2`            | The action is not supported for that service type                           |
| other non-zero | The action was run and failed, with the reason on stderr                    |

Exit code `2` separates "no backend can run this" from "the backend ran it and it failed".
It is what the runner exits with when the action has no template in `system.toml`,
when the service type has no plugin file,
and when the plugin itself exits `2`.
When an action is rejected this way by the init system,
the error message lists the actions `system.toml` does define.

:::note
Only the plugin path reports the reason of a failure.
The init system backend runs its templates with their output discarded,
so a failed `systemctl` gives no reason beyond its exit code.
:::

### Validating the arguments

`tedge service` runs as root, so every argument is checked before any backend is selected,
and all execution is argv-based: no argument is ever interpolated into a shell command line.

| Argument     | Rule                                                    |
|--------------|-------------------------------------------------------------|
| action       | `[a-z][a-z0-9_-]*`. Neither a space nor a leading `-`   |
| service name | Non-empty and not starting with `-`                     |
| service type | `[a-z0-9_-]+`, so it cannot escape the plugin directory |

A rejected argument fails the command without any backend being invoked.

## Plugin

* A plugin must be an executable file at `<plugin-dir>/<service-type>`,
  named exactly after the service type it handles, with no extension.
* It is invoked as `<plugin> <action> <service-name>`,
  where `<action>` is `start`, `stop`, `restart`, or any action the plugin defines itself.
* An action name is a single lowercase token, matching `[a-z][a-z0-9_-]*`.
  The plugin receives the name exactly as the service declared it on its
  [`cmd/<action>` topic](./agent/service-commands.md#actions),
  and never has to accept another spelling of it.
* The plugin decides which actions it supports.
  Nothing declares them to the runner.
* The plugin must return an appropriate exit code:
    * `0`: the action succeeded
    * `2`: this action is not supported by this plugin, reserved for this meaning
    * any other non-zero value: the action failed, with the reason on stderr
* The plugin can print anything on stdout and stderr.
  When the caller is `tedge-agent`, both end up in the operation log of the command.
  The only exception is the `:::begin-tedge:::` and `:::end-tedge:::` markers, which
  [update the state of a command](./agent/operation-workflow.md#next-step-determined-by-script-output):
  the runner drops the lines holding them,
  so that the output of a plugin is never read as a state update.

:::caution
Every plugin directory must stay owned by `root` and must not be writable by the `tedge` user.
`tedge service` runs the plugin as root,
so a writable directory would let that user run arbitrary code as root.
Packaging creates the shipped directory with mode `755`,
and deliberately leaves it out of the files it gives to the `tedge` user.
:::

## Example plugin

A plugin for the `container` service type,
handling `start`, `stop` and `restart`,
plus the custom actions `pause` and `unpause`.

```sh title="file: /usr/share/tedge/service-plugins/container"
#!/bin/sh
# Service plugin for the "container" service type.
# Invoked by `tedge service` as: container <action> <service-name>
#
# Exit codes:
#   0   success
#   1   action failed (reason on stderr)
#   2   action not supported by this plugin (reserved)
set -eu

ACTION="$1"
NAME="$2"

case "$ACTION" in
    start)   docker start   "$NAME" ;;
    stop)    docker stop    "$NAME" ;;
    restart) docker restart "$NAME" ;;
    pause)   docker pause   "$NAME" ;;
    unpause) docker unpause "$NAME" ;;
    *)
        echo "container plugin: unsupported action '$ACTION'" >&2
        exit 2
        ;;
esac
```

The plugin has to be executable:

```sh
sudo chmod 755 /usr/share/tedge/service-plugins/container
```

It can then be used on its own:

```sh
sudo tedge service pause nodered --service-type container
```
