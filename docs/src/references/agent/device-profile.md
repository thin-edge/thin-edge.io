---
title: Configuration Management
tags: [Reference, Device Profile, Firmware Management, Software Management, Configuration Management]
sidebar_position: 6
description: Device profile API proposal
---

# Device profile

A device profile defines any desired combination of a firmware, software and associated configurations to be installed on a device.
Device profiles are used to get a fleet of devices into a consistent and homogenous state by having the same set of firmware,
software and configurations installed on all of them.

The `tedge-agent` handles `device_profile` operations as follows:

* Declares `device_profile` support by sending the capability message to `<root>/<device-topic-id>/cmd/device_profile` topics
* Subscribes to `<root>/<device-topic-id>/cmd/device_profile/+` to receive `device_profile` commands.
* The `device_profile` command payload is an ordered list of modules representing any firmware, software and configuration combination.
* The agent processes each module one by one in the same order, triggering sub-operations for the respective module type.
* One successful installation of all the modules, the applied profile information is published to the same capability topic.
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
      "module": "firmware",
      "include": true,
      "name": "core-image-tedge-rauc",
      "remoteUrl": "https://abc.com/some/firmware/url",
      "version": "20240430.1139"
    },
    {
      "module": "software",
      "include": true,
      "name": "collectd",
      "version": "latest",
      "type": "apt",
      "action": "install"
    },
    {
      "module": "configuration",
      "include": true,
      "type": "collectd.conf",
      "remoteUrl":"https://abc.com/some/collectd/conf",
      "path": "/etc/collectd/collectd.conf"
    },
    {
      "module": "software",
      "include": true,
      "name": "jq",
      "version": "latest",
      "type": "apt",
      "action": "install"
    }
  ]
}'
```

The profile definition in the payload is an array of modules.
Each module could be a firmware, software or configuration.
The modules are installed/applied in the order in which they are defined in the profile definition.
There is no restriction on the order of modules in a profile and hence can be defined in any preferred order.
For example, additional software can be installed or configurations updated before the firmware is updated.

The `"include"` field is optional and the value is true, by default.
It can be used to exclude already installed modules when the same profile is reapplied on a device after a partial failure.

When the profile is applied, `tedge-agent` is free to group modules of the same kind that are listed sequentially,
to optimize the execution of that operation.
For example, if 3 software modules are defined sequentially, the agent could group them into an `updateList`
so that they can be installed in one-go using the `update-list` API of software plugins.

## Tedge agent handling device profile commands

The `tedge-agent` handless `device_profile` commands using the workflow definition at `/etc/tedge/operations/device_profile.toml`.
This workflow definition handles each module type using sub-operation workflows defined for that type.
For example, the firmware module is installed by triggering a `firmware_update` sub-command
which in turn uses the `firmware_update` workflow for that operation execution.
Similarly software modules are installed with `software_update` subcommands and
configuration updates are applied using `config_update` subcommands.
These subcommands are triggered for each module defined in the profile definition in that order.

### On success

Once all the modules in the profile are successfully installed, the successful status is published

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

On failure, the `device_profile` operation is aborted at the module that caused the failure.
The remaining modules in the profile are not installed and no attempt is made to rollback the already installed modules either.
If the sub-operations support rollbacks at the sub-operation level, it is performed for the failed module.

For example, if a profile includes a firmware, 2 software and 1 configuration updates in that sequence,
if the failure happens during the second software update, no rollback is performed at the overall `device_profile` operation level,
or even for that failed software update, unless the `software_update` workflow for that software type supports rollbacks.
In that case the firmware and 1st software would remain installed, with the failed software update and last config update skipped.

The `device_profile` operation itself is marked failed as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile/1234' '{
  "status": "failed",
  "reason": "Installation of software module: `jq` failed. Refer to operation logs for further details."
  ... // Other input fields
}'
```

The same profile can be applied again, by excluding all the modules(`"include": false`) before the failed `jq` module as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main///cmd/device_profile/1234' '{
  "status": "init",
  "name": "prod-profile",
  "version": "v2",
  "profile": [
    {
      "module": "firmware",
      "include": false,
      "name": "core-image-tedge-rauc",
      "remoteUrl": "https://abc.com/some/firmware/url",
      "version": "20240430.1139"
    },
    {
      "module": "software",
      "include": false,
      "name": "collectd",
      "version": "latest",
      "type": "apt",
      "action": "install"
    },
    {
      "module": "configuration",
      "include": false,
      "type": "collectd.conf",
      "remoteUrl":"https://abc.com/some/collectd/conf",
      "path": "/etc/collectd/collectd.conf"
    },
    {
      "module": "software",
      "include": true,
      "name": "jq",
      "version": "latest",
      "type": "apt",
      "action": "install"
    }
  ]
}'
```

# Cumulocity operation mapping

TBD