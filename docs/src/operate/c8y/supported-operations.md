---
title: Supported Operations
tags: [Operate, Cumulocity, Operation]
description: Declaring and using custom operations
---

## Concepts

### Device operations

IoT devices often do more than just send data to the cloud. They also do things like:

* receive triggers from the operator
* reboot on demand
* install or remove software

These operations are supported by [Cumulocity IoT](https://cumulocity.com/guides/reference/device-management-library) and other cloud providers.
When such an operation is triggered from the cloud, the cloud mapper (e.g: `tedge-mapper-c8y`) processes that request.

The Cumulocity mapper treats the following operations as inbuilt operations and converts those into their equivalent tedge commands:

| Operation | Cumulocity Operation Type | Tedge Command |
| ----------|---------------------------|---------------|
| Device restart | `c8y_Restart` | `te/<device-topic-id>/cmd/restart` |
| Software update | `c8y_SoftwareUpdate` | `te/<device-topic-id>/cmd/software_update` |
| Configuration retrieval | `c8y_UploadConfigFile` | `te/<device-topic-id>/cmd/config_snapshot` |
| Configuration update | `c8y_DownloadConfigFile` | `te/<device-topic-id>/cmd/config_update` |
| Log retrieval | `c8y_LogfileRequest` | `te/<device-topic-id>/cmd/log_upload` |
| Firmware update | `c8y_Firmware` | `te/<device-topic-id>/cmd/firmware_update` |

Another process like the `tedge-agent` or an external plugin may process these mapped tedge commands.
The `tedge-agent` currently supports all the above mentioned inbuilt operations out-of-the-box.

For all other operation types, the mapper can execute a custom operation plugin if one is configured.

The `Supported Operations API` of the Cumulocity mapper can be used to add support for these custom operations,
or when the user wants to handle any of the inbuilt operations differently than how the `tedge-agent` handles it.

### Supported Operations API

The Supported Operations API utilises the file system to add or remove operations.
An operation file placed in `/etc/tedge/operations/c8y` indicates that
an operation with that name is supported by the tedge device on Cumulocity.
For e.g, an empty file named `c8y_Restart` in this directory represents that
the tedge device supports Cumulocity device restart operation.

The aggregated list of all the operation files in this directory represents the [Cumulocity supported operations list](https://cumulocity.com/guides/reference/device-management-library/#announcing-capabilities) of that device.
Whenever a new operation file is added to 

Similarly, an operation file at `/etc/tedge/operations/c8y/<child-device-xid>` indicates that
the child device with the given external id `<child-device-xid>` supports that operation.

## How to use Supported Operations

### Listing supported operations

The Cumulocity supported operations list of the tedge device can be retrieved by listing all the files in the `/etc/tedge/operations/c8y` directory.

```sh
ls -l /etc/tedge/operations/c8y
```

```text title="Output"
-rw-r--r-- 1 tedge tedge 0 Jan 01 00:00 c8y_Restart
```

Similarly, one can list all the currently supported operations for a child device as follows:

```sh
ls -l /etc/tedge/operations/c8y/<child-device>
```

```text title="Output"
-rw-r--r-- 1 tedge tedge 0 Oct 26 11:24 c8y_LogfileRequest
```

### Adding new operations

To add new operation we need to create new file in the `/etc/tedge/operations/c8y` directory.
For e.g, to enable device restart operations from Cumulocity, a device must declare `c8y_Restart` as a supported operation.
This can be done by publishing the following operation capability MQTT message:

```sh
tedge mqtt pub -r 'te/device/main///cmd/restart' '{}'
```

The mapper, in response to this tedge capability message, creates a `c8y_Restart` operation file at `/etc/tedge/operations/c8y`.

:::note
The `tedge-agent` sends these capability messages automatically for all the inbuilt operations, when it starts up.
:::

Operation files can also be created manually at this directory:

```sh
sudo -u tedge touch /etc/tedge/operations/c8y/c8y_Restart
```

:::note
We are using `sudo -u` to create the file because we want to make sure that the file is owned by `tedge` user.
:::

Whenever a new operation file is added to this directory, the supported operations list for that device is updated
by aggregating all the operation file names in that directory and this updated list is sent to the cloud
using the [SmartRest 114 message](https://cumulocity.com/guides/reference/smartrest-two/#114).

:::warning
Updating the supported operations list by manually adding files to the operations directory is currently deprecated
and will be removed in an upcoming release.
It is advised to use the MQTT capability message mechanism for the same.
:::

Similarly, a child device can declare that it supports restart operation by publishing the following operation capability message:

```sh
tedge mqtt pub -r 'te/device/<child-device>///cmd/restart' '{}'
```

Operation files can also be placed manually in the child device operations directory at `/etc/tedge/operations/c8y/<child-device-xid>`.
But, unlike the main device, the supported operations list of child devices are not aggregated and published whenever this directory is updated,
but only when such a capability message is received from a child device.

### Removing supported operations

To remove a supported operation for a %%te%% device, the corresponding operation file must be removed from the `/etc/tedge/operations/c8y` directory. eg:

```sh
sudo rm /etc/tedge/operations/c8y/c8y_Restart
```

Now the operation will be automatically removed from the list and the list will be sent to the cloud.

:::warning
Dynamic removal of an operation from the supported operation list is not supported for child devices.
:::

## Working with custom operations

The `Supported Operations API` can also be used to add support for custom operations beyond the inbuilt ones.
For e.g: Cumulocity supports the `c8y_Command` that enables execution of shell commands on the device from the cloud.
The same operation files can be used to define how the mapper should handle these operations when triggered from the cloud.

### Adding new custom operation

An operation file must be created with the name of the operation:

```sh
sudo -u tedge touch /etc/tedge/operations/c8y/c8y_Command
```

...with the following content:

```toml title="file: /etc/tedge/operations/c8y/c8y_Command"
[exec]
  topic = "c8y/s/ds"
  on_message = "511"
  command = "/etc/tedge/operations/command"
  timeout = 10
```

In this example, the mapper is configured to pick up the `c8y_Command` operation message received on `c8y/s/ds` topic
with the SmartRest template message prefix `511`.
When such a message is received, the operation plugin located at `/etc/tedge/operations/command` would be executed.
The operation is configured to `timeout` after 10 seconds, to avoid it from running for too long/forever.

:::note
The operation file needs to be readable by %%te%% user - `tedge` - and should have permissions `644`.
The filename **MUST** only use alphanumeric and underscore characters, e.g. `A-Z`, `a-z`, `0-9` and `_`.
You cannot use a dash "-", or any other characters in the filename, otherwise the custom operation definition will be ignored.
:::

:::note
The `timeout` that is configured will be in seconds.
If a custom operation is not configured with a `timeout` value, then it will use default `timeout`,.i.e. 3600 seconds.
If the operation does not complete within that specified `timeout` period, then the operation will be stopped/killed, and marked as failed in the cloud.
:::

Here is a sample operation plugin that can handle the `c8y_Command`:

```sh title="file: /etc/tedge/operations/command"
#!/bin/bash
# Parse the smart rest message, ignore the first two field, and everything afterwards is the command
COMMAND="${1#*,*,}"

# Check if command is wrapped with quotes, if so then remove them
if [[ "$COMMAND" == \"*\" ]]; then
    COMMAND="${COMMAND:1:-1}"
fi

# Execute command
bash -c "$COMMAND"
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
    echo "Command returned a non-zero exit code. code=$EXIT_CODE" >&2
fi

exit "$EXIT_CODE"
```

This script is invoked with the received SmartREST message (`511,DeviceSerial,<shell-command>`).
This simple script parses the third field of the received SmartREST message and executes that command.
If it exits with the status code `0`, a successful message with the stdout content will be reported to Cumulocity.
If it exits with a non-zero code, a failure message with the stderr content will be sent out.

:::note
The command will be executed with `tedge` permission level.
So, most of the system level commands will not work.
:::

The operation files for inbuilt operations can also be defined in this format to override
the inbuilt handling of those operations by the mapper, which just converts those to their equivalent tedge commands.
For e.g: if the tedge device wants to handle the `c8y_Restart` operation differently than how the `tedge-agent` handles it,
the `c8y_Restart` operation file for the tedge device can be defined in a similar manner.

This same custom operation file mechanism can be used for the child devices as well,
to either add support for additional operations or override the inbuilt ones,
as long as that operation can be fully handled by executing a configured `command` from the tedge device itself.
For e.g: if a child device is incapable of receiving and processing the tedge `restart` command via MQTT,
but can be restarted directly from the tedge device via some remote restart commands,
the `c8y_Restart` operation file for the child device can be defined to invoke those remote restart commands.

### List of currently supported operations parameters

* `topic` - The topic on which the operation will be executed.
* `on_message` - The SmartRest template on which the operation will be executed.
* `command` - The command to execute.
* `result_format` - The expected command output format: `"text"` or `"csv"`, `"text"` being the default.

:::info
The `command` parameter accepts command arguments when provided as a one string, e.g.

```toml title="file: /etc/tedge/operations/c8y/c8y_Command"
[exec]
  topic = "c8y/s/ds"
  on_message = "511"
  command = "python /etc/tedge/operations/command.py"
  timeout = 10
``` 

Arguments will be parsed correctly as long as following features are not included in input: operators, variable assignments, tilde expansion, parameter expansion, command substitution, arithmetic expansion and pathname expansion. 

In case those unsupported shell features are present, the syntax that introduce them is interpreted literally.

Be aware that SmartREST payload is always added as the last argument. The command presented above will actually lead to following code execution

```bash
python /etc/tedge/operations/command.py $SMART_REST_PAYLOAD
```
:::