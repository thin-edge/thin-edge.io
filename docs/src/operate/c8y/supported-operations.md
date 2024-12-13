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
using the [SmartREST 114 message](https://cumulocity.com/docs/smartrest/mqtt-static-templates/#114).

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

### Creating new custom operation

Custom operations can be triggered on the device by SmartREST message or JSON over MQTT.
In both case the behavior of thin-edge is defined by the operation configuration file that tells to which messages the device must react and how.
* With SmartREST, the SmartREST code of the message determines the operation to execute and  the SmartREST payload is passed unchanged to a script to which the command is delegated.
* With JSON over MQTT the name of operation to execute is given by the JSON payload of the message and the operation is delegated by the agent running on the target device (which can be the main device of a child device ), possibly with some transformation of the JSON payload.

:::note
The operation file needs to be readable by %%te%% user - `tedge` - and should have permissions `644`.
The filename **MUST** follow the Cumulocity naming constraints on operation names: i.e. only use alphanumeric and underscore characters, e.g. `A-Z`, `a-z`, `0-9` and `_`. Alternatively, it can have `.template` extension for a operation template file.
You cannot use a dash "-", or any other characters in the filename, otherwise the custom operation definition will be ignored.
:::

#### Create operation delivered via SmartREST message

An operation file must be created with the name of the operation:

```sh
sudo -u tedge touch /etc/tedge/operations/c8y/c8y_Command
```

... with the definition containing `on_message` field:

```toml title="file: /etc/tedge/operations/c8y/c8y_Command"
[exec]
topic = "c8y/s/ds"
on_message = "511"
command = "/etc/tedge/operations/command"
timeout = 10
```

The mapper is configured to pick up the `c8y_Command` operation message received on `c8y/s/ds` topic with the SmartREST template message prefix `511`. When such a message is received, the operation plugin located at `/etc/tedge/operations/command` would be executed. The operation is configured to `timeout` after 10 seconds, to avoid it from running for too long/forever.

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

This script is invoked with the received SmartREST message (`511,<device-xid>,<shell-command>`).
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

##### Configuration parameters

* `topic` - The topic on which the operation will be executed.
* `on_message` - The SmartREST template on which the operation will be executed.
* `command` - The command to execute.
* `result_format` - The expected command output format: `"text"` or `"csv"`, `"text"` being the default.
* `timeout` - The time in seconds after which the operation will be stopped/killed and marked as failed in cloud, by default it is 3600 seconds.

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

#### Create operation delivered via Cumulocity JSON over MQTT  

There are two approaches for handling custom operation via JSON over MQTT.
* the commands can be processed by a user-provided script
* the commands can be processed by the agent following a user-provided workflow

The first one can only be used with main device, while the second one extends it to child devices. Here is a sample of incoming JSON over MQTT message with custom operation fragment on topic `c8y/devicecontrol/notifications`: 

```json
{
  "delivery": {
    "log": [],
    "time": "2024-07-22T10:26:31.457Z",
    "status": "PENDING"
  },
  "agentId":"201802315",
  "creationTime":"2024-09-02T19:56:01.313Z",
  "deviceId":"201802315",
  "id":"1800380",
  "status":"PENDING",
  "c8y_Command":{
    "text":"echo helloworld"
  },
  "description":"Execute command",
  "externalSource":{
    "externalId":"test-device",
    "type":"c8y_Serial"
  }
}
```

The first approach assumes that user will create the operation file with the name of the operation:

```sh
sudo -u tedge touch /etc/tedge/operations/c8y/c8y_Command
```

... with the definition containing `on_fragment` field:

```toml title="file: /etc/tedge/operations/c8y/c8y_Command"
[exec]
topic = "c8y/devicecontrol/notifications"
on_fragment = "c8y_Command"
command = "/etc/tedge/operations/command ${.payload.c8y_Command.text}"
skip_status_update = false
```

The mapper is configured to pick up the message received on the topic provided in the operation file field `topic` (if the field is not provided, the default topic `c8y/devicecontrol/notifications` will be used). Mapper checks if the received message contains the value of `on_fragment`. Then, the operation plugin located at `/etc/tedge/operations/command` will be executed with the arguments provided in message payload (in this example `${.payload.c8y_Command.text}` will be replaced with `echo helloworld`).

:::info
Users can provide their own custom topic in the operation file:

```toml title="file: /etc/tedge/operations/c8y/c8y_Command"
[exec]
topic = "${.bridge.topic_prefix}/custom/topic"
on_fragment = "c8y_Command"
command = "/etc/tedge/operations/command ${.payload.c8y_Command.text}"
skip_status_update = false
```

Instead of providing `topic_prefix` manually, you can use `${.bridge.topic_prefix}` template to derive it from the bridge config of current cloud instance.

The mapper will automatically subscribe to all custom topic provided in operation file under `topic` field when connecting to the cloud.  
:::

:::note
When `on_fragment` is provided with `on_message`, the mapper will ignore the operation.
:::

For the second approach, the user has to create a operation template file with `.template` extension, e.g.: 

```toml title="file: /etc/tedge/operations/c8y/c8y_Command.template"
[exec]
topic = "c8y/devicecontrol/notifications"
on_fragment = "c8y_Command"

[exec.workflow]
operation = "command"
input = "${.payload.c8y_Command.text}"
output = "${.payload.result}"
```

To indicate that devices are supporting the operation, the device must publish a command capability message where command name is derived from `exec.workflow.operation`:

```sh
tedge mqtt pub -r 'te/device/<name>///cmd/command' '{}'
```

:::note
If the workflow file with the same command name is defined, capability message will be sent automatically.
:::

On receiving, mapper creates a symlink at `/etc/tedge/operations/c8y/c8y_Command` (for main device) or `/etc/tedge/operations/c8y/<external_id>/c8y_Command` (for child device) to operation template file.

The mapper detects the change in the `/etc/tedge/operations/c8y` directory except for `.template` file. When the change is detected, the SmartREST `114` (supported operation) message is sent to Cumulocity.

After that, the received JSON over MQTT message is converted to %%te%% command, preserving all parameters provided in `input` field.
The payload also contains the parameters retrieved from the operation template file, notably `on_fragment` and `output`.

```text title="Topic"
te/device/<name>///cmd/command/c8y-mapper-1800380
```

```json5 title="Payload"
{
  "status": "init",
  "text": "echo helloworld",
  "c8y-mapper": {
    "on_fragment": "c8y_Command",
    "output": "${.payload.result}"
  }
}
```

:::info
An operation file definition can contain multiple `input` fields:

```toml title="file: /etc/tedge/operations/c8y/c8y_Command.template"
[exec]
topic = "c8y/devicecontrol/notifications"
on_fragment = "c8y_Command"

[exec.workflow]
operation = "command"
input.x = "${.payload.c8y_Command.text}"
input.y = { foo = "bar" }
```

Conversion to %%te%% command will contain both of them:  

```text title="Topic"
te/device/<name>///cmd/command/c8y-mapper-1800380
```

```json5 title="Payload"
{
  "status": "init",
  "text":"echo helloworld",
  "foo":"bar"
}
```

Make sure that provided input is JSON object, otherwise the operation execution will be skipped.  

:::

:::info

The `output` field is required for specific operations (e.g., `c8y_Command` and `c8y_RelayArray`) that need to send additional parameters to the cloud to complete an operation successfully.
These parameters are extracted from the operation payload using the values specified in the `output` field.
This field accepts either a `String` or an `Array` value.
Placeholders in the output will be replaced with the actual payload values and appended as parameters to the SmartREST `503`/`506` messages.

#### Example 1: c8y_Command

For the `c8y_Command` operation, the execution output is stored in the `result` field. Here is an example JSON payload when it is successful:
```json5 title="c8y_Command"
{
  "c8y-mapper": {
    "on_fragment": "c8y_Command",
    "output": "${.payload.result}"
  },
  "command":"echo helloworld",
  "result":"helloworld",
  "status": "successful"
}
```

This message is converted to the following SmartREST message:
```
503,c8y_Command,helloworld
```

#### Example 2: c8y_RelayArray

For the c8y_RelayArray operation, the output is an array. Here is an example JSON payload when it is successful:

```json5 title="c8y_RelayArray"
{
  "c8y-mapper": {
    "on_fragment": "c8y_RelayArray",
    "output": "${.payload.states}"
  },
  "command":"echo helloworld",
  "states":["OPEN","CLOSED"],
  "status": "successful"
}
```

This message is converted to the following SmartREST message:

```
503,c8y_RelayArray,OPEN,CLOSED
```

:::

After conversion, the `tedge-agent` handles custom operation commands using the workflow definition at `/etc/tedge/operations/command.toml`.
Workflow definition must be created by the user.

Here is a sample workflow for `C8y_Command`:

```toml
operation = "command"

[init]
action = "proceed"
on_success = "executing"

[executing]
script = "/etc/tedge/operations/command ${.payload.c8y_Command.text}"
on_success = "successful"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
```

##### Configuration parameters
Available in all operation files with the JSON over MQTT input:
* `topic` - The topic on which the operation will be executed (by default: `c8y/devicecontrol/notifications`).
* `on_fragment` - Used to check if the mapping file matches the input payload.

Available only in non-template operation file definition: 
* `command` - The command to execute.
* `skip_status_update` - Optional boolean value that decide whether or not mapper should send operation status update messages (SmartREST messages `501`-`503`/`504`-`506`). The default value is `false`.

Available only in the template operation file definition:
* `workflow.operation` - The command name that will trigger workflow execution.
* `workflow.input` - The JSON object input that can be used in the workflow.
* `workflow.output` - The JSON object that is converted to the additional parameters of the SmartREST successful status update messages (`503`/`506`)
