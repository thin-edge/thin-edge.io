---
title: Configuration Management
tags: [Reference, Device Profile, Firmware Management, Software Management, Configuration Management]
sidebar_position: 6
description: Device profile API proposal
---

# Device profile

A device profile defines any desired combination of a firmware, software and associated configurations to be installed on a device.
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

* Ability to control the order of execution of the operations defined in the command input
* Ability to apply 

# Device profile capability

A device that supports device profile operation must declare that capability by publishing the following message:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile' '{
  "name": "prod-profile",
  "version: "v1"
}'
```

If the current profile information is not known upfront, publish an empty JSON (`{}`) instead.
The firmware information, supported software and configuration types must be declared separately with their respective capability messages.

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
  "profile": [
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
operations = [
    { operation = "firmware_update", target_operation="firmware_update" }
    { operation = "software_update", target_operation="custom_software_update" }
    { operation = "config_update" }
]
on_exec = "awaiting_sub_operation"
on_success = "successful"
on_error = { status = "failed", reason = "fail to apply device profile"}

[awaiting_sub_operation]
action = "await-operation-completion"
on_success = "executing"
on_error = { status = "failed", reason = "fail to update configuration"}

#
# End states
#
[successful]
action = "cleanup"

[failed]
action = "cleanup"
```

* The workflow just proceeds to the `scheduled` state from the `init` state
* The order of operation execution is finalized in the `scheduled` state.
  If the `builtin` action is specified in this state, the default order as defined in the input is used.
  This order can be modified using a `script` action, if desired.
  The updated list is captured into a `updated_profile` field with the starting `current_index` value.
  The `current_index` value specifies the next item in the list to be processed by the `executing` state.
  By default, this `current_index` value starts with `0`.
  This index value is incremented as the operation execution proceeds.
  This index can also be used to skip the first `n` items in the list, especially when the same profile is reapplied.
* The `target_operation` to be invoked for each item in the input is defined in the `executing` state.
  If a `target_operation` is not explicitly specified, then the default workflow for the `operation` value itself is used.
  The sub-operation execution starts from the item corresponding to the `current_index` value.
  As soon as the sub-operation is triggered, the workflow moves to the `awaiting_sub_operation` state defined as the `on_exec` target.
* In the `awaiting_sub_operation` state, workflow just waits monitoring the state of the sub-operation completion.
  Once the sub-operation is successful, the workflow moves back to the `executing` state with the index value incremented,
  so that the next item can be applied.
  * In case of a failure, the workflow moves to the `on_error` target state, keeping the `current_index` value intact,
    so that the item that caused the failure can be easily identified.
* Once the `updated_profile` list is exhausted in the `executing` state, the workflow moves to its `on_success` target state.


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
tedge mqtt pub -r 'te/device/main///cmd/device_profile' '{
  "name": "prod-profile",
  "version: "v2"
}'
```

### On failure

On failure, the `device_profile` operation is aborted at the operation that caused the failure.
The remaining operations in the profile are executed and no attempt is made to rollback the already completed operations either.
If the sub-operations support rollbacks at the sub-operation level, it is performed for the failed operation.

For example, if a profile includes a firmware, 2 software and 1 configuration updates in that sequence,
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

TBD