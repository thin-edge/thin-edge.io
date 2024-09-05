---
title: Device profiles
tags: [Reference, Device Profile, Firmware Management, Software Management, Configuration Management]
sidebar_position: 6
description: Device profile API proposal
---

A device profile defines any desired combination of firmware, software and associated configurations to be installed on a device.
Device profiles are used to get a fleet of devices into a consistent and homogeneous state by having the same set of firmware,
software and configurations installed on all of them.

The `tedge-agent` handles `device_profile` operations as follows:

* Declares `device_profile` support by sending the capability message to `<root>/<device-topic-id>/cmd/device_profile` topics
* Subscribes to `<root>/<device-topic-id>/cmd/device_profile/+` to receive `device_profile` commands.
* The `device_profile` command payload is an ordered list of firmware, software and configuration update operations.
* The agent processes each operation one by one, triggering sub-operations for the respective operation.
* On successful installation of all the modules, the applied profile information is published to the same capability topic.
* No rollback is performed on partial failures unless the subcommand for the failed module can rollback that single module.

# Why device profile FAQ

**Q1: Why do we need device profile when firmware, software and configuration management already exists**

Performing and managing these operations individually for a long list of software and configuration items would be cumbersome,
especially when performed on a large fleet of devices as each operation will have to be monitored and managed separately.
Grouping them together into a single operation reduces that overhead considerably as you just have one operation to monitor
instead of multiple.

**Q2: Why not model each device profile update as a firmware update that includes the desired software and configurations as well**

Although this is a more robust approach, it is not feasible on all kinds of devices,
especially the ones that does not support delta firmware updates.
On such devices, pushing the entire firmware binary for each iterative change would considerably increase the binary size overhead.

# Requirements

* Ability to override the order of execution of the operations defined in the command input.
* Ability to dynamically control the next operation to be executed during the workflow execution.
* Provide an option to add any custom rollback step into the workflow, when feasible,
  with sufficient info available to the rollback logic like which operation failed, which ones completed and which ones are pending.

# Device profile capability

A device that supports device profile operation must declare that capability
by publishing an empty JSON message to the `device_profile` command metadata topic as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile' '{}'
```

If a profile is applied on the factory image itself, this information can be published to the corresponding `twin` topic,
so that the existing profile information is propagated to the cloud as well.

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///twin/device_profile' '{
  "name": "prod-profile",
  "version": "v1"
}'
```

If the current profile information is not known upfront, this step can be skipped.

When a new profile is applied, this twin value is updated with the applied profile's `name` and `version`.

# Device profile command

Once the `device_profile` capability is declared, the device can receive `device_profile` commands
by subscribing to `<root>/<device-topic-id>/cmd/device_profile/+` MQTT topics.
For example, subscribe to the following topic for the `main` device:

```sh te2mqtt formats=v1
tedge mqtt sub 'te/device/main///cmd/device_profile/+'
```

A `device_profile` command with `id` "1234" is triggered as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile/1234' '{
  "status": "init",
  "name": "prod-profile",
  "version": "v2",
  "operations": [
    {
      "operation": "firmware_update",
      "skip": false,
      "payload": {
        "name": "core-image-tedge-rauc",
        "remoteUrl": "https://abc.com/some/firmware/url",
        "version": "20240430.1139"
      }
    },
    {
      "operation": "software_update",
      "skip": false,
      "payload": {
        "updateList": [
          {
            "type": "apt",
            "modules": [
              {
                "name": "c8y-command-plugin",
                "version": "latest",
                "action": "install"
              },
              {
                "name": "collectd",
                "version": "latest",
                "type": "apt",
                "action": "install"
              }
            ]
          }
        ]
      }
    },
    {
      "operation": "config_update",
      "skip": false,
      "payload": {
        "type": "collectd.conf",
        "remoteUrl":"https://abc.com/some/collectd/conf",
        "path": "/etc/collectd/collectd.conf"
      }
    },
    {
      "operation": "software_update",
      "skip": false,
      "payload": {
        "updateList": [
          {
            "type": "apt",
            "modules": [
              {
                "name": "jq",
                "version": "latest",
                "action": "install"
              }
            ]
          }
        ]
      }
    }
  ]
}'
```

The profile definition in the payload is an array of operations.
Each operation could be a `firmware_update`, `software_update` or `config_update`.
The operations are executed in the order in which they are defined in the profile definition, by default.
There is no restriction on the order of modules in a profile and hence can be defined in any preferred order.
For example, additional software can be installed or configurations updated before the firmware is updated.
This default execution order can also be overridden in the workflow definition, by updating the order in the `scheduled` state.

The `"skip"` field is optional and the value is `false`, by default.
It can be used to skip any operations during the development/testing phase, without fully deleting the entry from the profile.

## Tedge agent handling device profile commands

The `tedge-agent` handles `device_profile` commands using the workflow definition at `/etc/tedge/operations/device_profile.toml`.
This workflow definition handles each module type using sub-operation workflows defined for that type.
For example, the firmware module is installed by triggering a `firmware_update` sub-command
which in turn uses the `firmware_update` workflow for that operation execution.
Similarly software modules are installed with `software_update` subcommands and
configuration updates are applied using `config_update` subcommands.
These subcommands are triggered for each module defined in the profile definition in that order.

Here is a sample device profile workflow:

```toml
operation = "device_profile"

[init]
action = "proceed"
on_success = "scheduled"

# Sort the inputs as desired
[scheduled]
script = "/etc/tedge/operations/device_profile.sh ${.payload.status} ${.payload}"
on_success = "executing"
on_error = { status = "failed", reason = "fail to sort the profile list"}

[executing]
action = "proceed"
on_success = "next_operation"

[next_operation]
iterate = "${.payload.operations}"
on_next = "apply_operation"
on_success = "successful"
on_error = { status = "failed", reason = "Failed to compute the next operation to be executed" }

[apply_operation]
operation = "${.payload.@next.operation.operation}"
input = "${.payload.@next.operation.payload}"
on_exec = "awaiting_operation"

[awaiting_operation]
action = "await-operation-completion"
on_success = "next_operation"
on_error = "rollback"

[rollback]
action="proceed"
on_success = { status = "failed", reason = "Device profile application failed" }
on_error = { status = "failed", reason = "Rollback failed" }

#
# End states
#
[successful]
action = "cleanup"

[failed]
action = "cleanup"
```

* The workflow just proceeds to the `scheduled` state from the `init` state
* The order of operation execution must be finalized before the `executing` state
  and the `scheduled` state is an ideal candidate for that.
  If the `builtin` action is specified in this state, the default order as defined in the input is used.
  This order can be overridden by the user using a `script` action, if desired.
  The script is expected to return the updated `operations` list which replaces the original list
  and then proceed to the `executing` state.
* The mandatory `executing` state simply passes the input to the `next_operation`.
* The `next_operation` state chooses the next operation to be executed from the list of `operations` in the input.
  When the built-in `iterator` action is used, the next operation is picked up sequentially from the `operations` list.
  The target operation is captured into a `@next_operation` object in the payload,
  with an `index` field representing the index position of that operation in the `operations` list, 
  along with that operation's `operation` and `payload` values as follows:

  ```json
  {
    "@next_operation": {
      "index": 0,
      "operation": "firmware_update",
      "payload": {
        "name": "core-image-tedge-rauc",
        "remoteUrl": "https://abc.com/some/firmware/url",
        "version": "20240430.1139"
      }
    },
    ... // Other fields in the incoming payload
  }
  ```

  If the `@next_operation` field is not present in the input payload.
  one is added with an initial `index` value of `0` and the corresponding `operation` and `payload` values.
  If the field already exists, the `index` value is incremented along with its `operation` and `payload` values.
  Once the next operation is successfully computed, the workflow moves to the `on_next` target.
  Once the `operations` list is exhausted (`index` value higher than its size),
  the profile application is deemed complete and the workflow proceeds to the `on_success` target.
  If the operation computation fails for some reason, then the workflow moves to the `on_error` target.
  This builtin iteration logic can be overridden using a `script` action which can manipulate the order in any manner, dynamically.
* The `apply_operation` state executes the sub-operation defined in the `@next_operation` field in the payload.
  The `input` to the sub-operation is also extracted from the `payload` field of the `@next_operation`.
  As soon as the sub-operation is triggered, the workflow moves to the `awaiting_operation` state defined as the `on_exec` target.
* In the `awaiting_operation` state, workflow just waits monitoring the state of the sub-operation completion.
  * Once the sub-operation is successful, the workflow must move back to the `next_operation` state,
    so that the next operation in the list can be applied.
  * In case of a failure, the workflow moves to the `on_error` target state,
    keeping the `@next_operation` value in the payload intact,
    so that the item that caused the failure can be easily identified using the its `index` value.
    Using the `index` value, all the operations that were previously applied can easily be identified
    by looking up all the lower index values in the `operations` list.
    This can come in handy for any rollback attempts if feasible.
    For e.g: if a profile consisted of a 4 inter-connected configuration updates and if the profile application failed
    during the 3rd configuration,
    a profile level rollback can be implemented by identifying the previously applied config update operations
    using the `index` value, and undoing them as well.
* The `rollback` state does nothing but just falls through to the `failed` state,
  as there is no built-in support for a profile level rollbacks.
  If such a rollback is feasible, this state must be overridden using a user provided `script` action.

### On success

Once all the operations in the profile are successfully completed, the successful status is published

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile/1234' '{
  "status": "successful",
  ... // Other input fields
}'
```

...and the current applied device profile information is updated by publishing the same to the capability topic as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///twin/device_profile' '{
  "name": "prod-profile",
  "version": "v2"
}'
```

### On failure

On failure, the `device_profile` operation is aborted at the operation that caused the failure.
The remaining operations in the profile are executed and no attempt is made to rollback the already completed operations either.
If the sub-operations support rollbacks at the sub-operation level, it is performed for the failed operation.

For example, if a profile includes firmware, 2 software packages and 1 configuration update in that sequence,
if the failure happens during the second software update, no rollback is performed at the overall `device_profile` operation level,
or even for that failed software update, unless the `software_update` workflow for that software type supports rollbacks.
In that case the firmware and 1st software would remain installed, with the failed software update and last config update skipped.
But, if the failure happens during the `firmware_update` itself, a rollback is most likely performed by that workflow,
as most `firmware_update` workflows support a robust rollback mechanism.

The `device_profile` operation itself is marked failed as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile/1234' '{
  "status": "failed",
  "reason": "Installation of software module: `jq` failed. Refer to operation logs for further details."
  ... // Other input fields
}'
```

The same profile can be applied again after fixing the issues that caused the failure,
and it is left to the individual sub-operations to determine whether
the same operation that was successfully applied in the last attempt must be reapplied or not.
The user can also manually skip any operation using the `skip` field.

# Cumulocity operation mapping

Cumulocity device profiles represent a combination of firmware, one or more software packages and configuration files,
represented by the `c8y_DeviceProfile` operation type.
Here is a sample `c8y_DeviceProfile` operation payload on the `c8y/devicecontrol/notifications` topic:

```json
{
  "delivery": {
    "log": [],
    "time": "2024-07-22T10:26:31.457Z",
    "status": "PENDING"
  },
  "agentId": "98523229",
  "creationTime": "2024-07-22T10:26:31.441Z",
  "deviceId": "98523229",
  "id": "523244",
  "status": "PENDING",
  "profileName": "prod-profile-v2",
  "description": "Assign device profile prod-profile-v2 to device TST_char_humane_exception",
  "profileId": "50523216",
  "c8y_DeviceProfile": {
    "software": [
      {
        "name": "c8y-command-plugin",
        "action": "install",
        "version": "latest",
        "url": " "
      },
      {
        "name": "collectd",
        "action": "install",
        "version": "latest",
        "url": " "
      }
    ],
    "configuration": [
      {
        "name": "collectd-v2",
        "type": "collectd.conf",
        "url": "https://t2373.basic.stage.c8y.io/inventory/binaries/88395"
      }
    ],
    "firmware": {
      "name": "core-image-tedge-rauc",
      "version": "20240430.1139",
      "url": "https://t2373.basic.stage.c8y.io/inventory/binaries/43226"
    }
  },
  "externalSource": {
    "externalId": "TST_char_humane_exception",
    "type": "c8y_Serial"
  }
}
```

There can only be one firmware entry in the device profile along with multiple software and configuration items.
Artifacts of each type are always grouped together and hence do not allow interleaving of different artifact types.
The payload does not enforce any clear order between artifact types either.

The mapping from this Cumulocity format to tedge JSON format is fairly straight-forward.
Each artifact type is mapped to the corresponding operation type in thin-edge
(e.g: `software` -> `software_update`, `configuration` -> `config_update` and `firmware` -> `firmware_update`).

Since the thin-edge payload is an ordered list of operations offering flexibility in defining them in any order,
the C8y payload is mapped to the equivalent tedge JSON format by applying an implicit order between the artifact types,
starting with the `firmware_update` operation followed by `software_update` and then `config_update`.
Since both `software` and `configuration` values are arrays with a defined order, it is maintained during the mapping as well.

The above payload, meant for the main device, is mapped to thin-edge JSON format as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile/523244' '{
  "status": "init",
  "name": "prod-profile",
  "operations": [
    {
      "operation": "firmware_update",
      "skip": false,
      "payload": {
        "name": "core-image-tedge-rauc",
        "version": "20240430.1139",
        "remoteUrl": "https://t2373.basic.stage.c8y.io/inventory/binaries/43226"
      }
    },
    {
      "operation": "software_update",
      "skip": false,
      "payload": {
        "updateList": [
          {
            "type": "apt",
            "modules": [
              {
                "name": "c8y-command-plugin",
                "version": "latest",
                "action": "install"
              },
              {
                "name": "collectd",
                "version": "latest",
                "type": "apt",
                "action": "install"
              }
            ]
          }
        ]
      }
    },
    {
      "operation": "config_update",
      "skip": false,
      "payload": {
        "type": "collectd.conf",
        "remoteUrl":"https://t2373.basic.stage.c8y.io/inventory/binaries/88395"
      }
    }
  ]
}'
```

Since Cumulocity device profiles do not contain any version information, it is omitted in the tedge JSON payload as well.

If the users want to change this implicit order of operation execution,
then they may enforce a different order in the `device_profile` workflow definition,
by overriding any state (e.g: `scheduled` state) before the workflow moves to the `executing` state.
