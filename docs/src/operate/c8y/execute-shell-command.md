---
title: Execute Shell Command
tags: [Operate, Operation, Cumulocity]
description: Executing a shell command on a device from Cumulocity
---

%%te%% supports the Cumulocity *Shell* operation (`c8y_Command`) out of the box,
which lets you run a shell command on a device from the cloud.

The commands are queued by Cumulocity and executed one after the other by the device,
and the combined standard output and standard error of the command is reported back as the operation result.

## Executing a command

1. Go to the *Device Management* application in Cumulocity

2. Find your device and open up its *Shell* tab

3. Enter the command to be executed, e.g. `uptime`, and click *Execute*

The operation transitions to `SUCCESSFUL` when the command returns a zero exit code,
and to `FAILED` otherwise.

The same operation can also be created using the Cumulocity REST API or the [go-c8y-cli](https://c8y.app/) cli tool.

```sh
c8y operations create --device "$DEVICE_ID" --template "{c8y_Command:{text:'uptime'}}"
```

## How it works

The Cumulocity operation is handled by two independent pieces:

* the `shell_execute` [operation workflow](../../references/agent/operation-workflow.md),
  which is a cloud-agnostic %%te%% command deployed by the **tedge-agent** to
  `/etc/tedge/operations/shell_execute.toml`
* the `c8y_Command` operation template,
  which is deployed by the **tedge-mapper-c8y** to `/etc/tedge/operations/c8y/c8y_Command.template`
  and maps the Cumulocity `c8y_Command` operation onto the `shell_execute` command

The `shell_execute` command can therefore also be triggered locally, without any cloud.

```sh
tedge mqtt pub -q 1 te/device/main///cmd/shell_execute/local-1234 '{"status":"init","command":"uptime"}'
```

The workflow follows the template pattern:
`shell_execute.toml.template` holds the definition shipped by %%te%%,
while `shell_execute.toml` is left untouched once you customise it,
so your changes survive an upgrade.
The **tedge-agent** restores `shell_execute.toml` from the template if the file is missing,
unless a `shell_execute.toml.disabled` marker exists.

The Cumulocity operation template is only created when missing:
delete `c8y_Command.template` and restart the **tedge-mapper-c8y**
to get the definition shipped by the installed version back.

## Configuration

|Property|Description|
|--|--|
|`shell.path`|The shell used to run the commands. Defaults to `/bin/sh`|
|`shell.max_output_size`|The maximum number of bytes of command output stored and reported back. Any output beyond that limit is discarded. Defaults to `15000`, just under what fits in a Cumulocity message|
|`shell.timeout`|The maximum duration of a command. Defaults to `10m`|
|`agent.enable.shell_execute`|Whether the **tedge-agent** deploys and executes the `shell_execute` command. Defaults to `true`|
|`c8y.enable.shell_execute`|Whether the `shell_execute` command is mapped to the Cumulocity `c8y_Command` operation. Defaults to `true`|

A command which has not completed within `shell.timeout` is terminated and the operation fails,
reporting the output collected so far.
The processes started by the command are terminated too,
unless they have moved to a process group or session of their own, e.g. using `setsid`.
The command is first sent `SIGTERM`, then `SIGKILL` if it is still running 60 seconds later.

```sh
tedge config set shell.timeout 30m
```

The output is collected on disk, under `data.path` (`/var/tedge` by default),
for the whole duration of the command.
Only the first `shell.max_output_size` bytes are stored,
the rest of the output being discarded,
so a command printing a lot cannot fill up the disk.
The command itself is not affected by the truncation and runs to completion.
The files of a command are kept under `data.path`, rather than `tmp.path`,
so the outcome of a command survives a reboot of the device triggered by the command itself.

:::caution
The operation completes as soon as the command exits,
even if the command started a background process.
Such a process should redirect its output, e.g. `my-daemon >/dev/null 2>&1 &`,
as it is sent `SIGPIPE`, which terminates it by default,
if it writes to the output it inherited from the command after the command completed.
:::

:::note
The `shell.*` settings are read by the `tedge-shell-plugin` process started by the workflow,
which uses the default configuration directory unless told otherwise.
If the **tedge-agent** runs with a `--config-dir` other than `/etc/tedge`,
set `TEDGE_CONFIG_DIR` in its environment as well,
so the plugin reads the settings from the same directory.
:::

The command is run as `<shell> -c "<command>"`, for instance:

```sh
tedge config set shell.path /bin/bash
```

To stop exposing the operation to Cumulocity, disable the feature and restart the mapper.

```sh
tedge config set c8y.enable.shell_execute false
```

```sh
sudo systemctl restart tedge-mapper-c8y
```

:::note
Disabling the feature stops the mapper from handling `c8y_Command` operations,
but it does not remove the `c8y_Command` operation file which has already been created
under `/etc/tedge/operations/c8y`.
Remove that file to also stop advertising the operation to Cumulocity.
:::

## Restarting the agent or the device

A command can restart the **tedge-agent**, or the device,
e.g. `sudo systemctl restart tedge-agent` or `sudo reboot`,
provided the user running the agent is allowed to do so.

The command is usually killed along with the agent, e.g. by systemd,
in which case its output is lost.
When the agent is back, the operation is reported as **failed**,
with a reason telling that the command has been interrupted,
as there is no way to tell whether the command did its job before being killed.
No exit code is reported, as the exit code of the command is lost when it is killed along with the agent.
A command which survives the restart of the agent is awaited,
and its actual outcome is reported.
In any case, the command is never run a second time.

Use a command which reports its own outcome before the restart,
e.g. by publishing an event, if you need to know how far it went.

When the interruption is expected,
set the result to be reported with `tedge-shell-plugin set-result` before the restart,
so the operation is reported with this result rather than as interrupted:

```sh
tedge-shell-plugin set-result --outcome successful && sudo tedge service restart tedge-agent
```

A failure can be reported the same way, with an optional reason:

```sh
tedge-shell-plugin set-result --outcome failed --reason 'Failed to do something' && sudo reboot
```

The reason is used as is as the failure reason of the operation.

The output printed by the command up to its interruption is reported as the operation result,
followed by a notice telling that it may be incomplete.
The output printed before `set-result` is written to disk before `set-result` returns,
while what is printed afterwards might be lost if the device is rebooted.
For the result to survive a reboot, `data.path` must be on a persistent partition,
failing which the command is reported as interrupted.
A command which completes anyway, e.g. because the restart failed,
is reported with its actual outcome.
The command finds its identifier in the `SHELL_EXECUTE_CMD_ID` environment variable,
which is where `set-result` reads it from.

A result set with `set-result` is flagged with `"result_set": true` in the output of the `collect` step,
next to `result` and, for a failure, `reason`.
A customised `shell_execute.toml` can use this flag to handle such a result differently,
e.g. using the `on_success` or `on_exit.1` handlers of the `collect` step,
into which the whole output is merged.

:::note
The `shell_execute` workflow runs the command as a
[background script](../../references/agent/operation-workflow.md#background-scripts),
moving to a `collect` state before the command is started.
This state is persisted, so the agent resumes the operation where it was left,
and waits for the command to complete before reporting its outcome.
:::

## Security considerations

The commands are executed by the **tedge-agent**, and therefore run as the user the agent runs as,
which is `tedge` by default.
Anybody who can create an operation for the device in Cumulocity can run
any command that this user is allowed to run, including the commands granted to it via `sudo`.

:::caution
On a default installation the `tedge` user is granted passwordless `sudo` for
`tedge`, `tedge-write` and the software management plugins, which is enough to obtain root.
**Treat the ability to create a `c8y_Command` operation for a device as equivalent to root access
on that device**, and restrict it in Cumulocity accordingly.

This operation is enabled by default, including on devices upgraded from a version
which did not provide it.
:::

If this is not acceptable for your deployment, either disable the feature,
or replace `/etc/tedge/operations/shell_execute.toml` with a workflow which only accepts
a restricted set of commands.

To turn the command off on the device altogether, disable it in the **tedge-agent** configuration.
The agent then neither deploys the workflow nor executes any `shell_execute` command.
Remove the workflow definition too, so the operation is no longer advertised.

```sh
sudo tedge config set agent.enable.shell_execute false
sudo rm -f /etc/tedge/operations/shell_execute.toml
sudo systemctl restart tedge-agent
```

Alternatively, disable the workflow with a marker file.
Note that removing `/etc/tedge/operations/shell_execute.toml` on its own is not enough:
the **tedge-agent** deploys it again on its next start
unless the marker file is present or `agent.enable.shell_execute` is `false`.

```sh
sudo touch /etc/tedge/operations/shell_execute.toml.disabled
sudo rm -f /etc/tedge/operations/shell_execute.toml
sudo systemctl restart tedge-agent
```

`c8y.enable.shell_execute` only controls the Cumulocity mapping:
with the command still enabled on the agent, it can be triggered locally over MQTT.

## Migrating from the tedge-command-plugin

The `c8y_Command` operation used to be provided by the
[tedge-command-plugin](https://github.com/thin-edge/tedge-command-plugin) community package
(and formerly by the `c8y-command-plugin`).
These packages are superseded by the built-in support, and how they are removed
depends on the package manager.

|Package manager|Behaviour|
|--|--|
|`apt` (deb)|The `tedge` package conflicts with them, so `apt-get install tedge` removes them, as does an update triggered from the cloud, which uses `apt-get install` too. A plain `apt-get upgrade` never removes a package: it holds the new `tedge` back instead, leaving the device on its current version. Use `apt full-upgrade`, or remove the community package first|
|`dnf`/`yum` (rpm)|The `tedge` package obsoletes them, so they are removed on update|
|`apk`|The community package has to be removed manually|

```sh
sudo apt-get remove tedge-command-plugin
```

```sh
sudo apk del tedge-command-plugin
```

The community package owns both `/etc/tedge/operations/shell_execute.toml`
and `/etc/tedge/operations/c8y/c8y_Command.template`,
so as long as it is installed, its own definitions take precedence over the built-in ones.
Removing the package removes those files,
which the **tedge-agent** and the **tedge-mapper-c8y** deploy again when they are restarted.

```sh
sudo systemctl restart tedge-agent tedge-mapper-c8y
```
