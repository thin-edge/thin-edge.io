---
title: Software Management
tags: [Reference, Agent, Software Management]
sidebar_position: 3
description: Details of the API to manage software on a device
---

%%te%% software management is implemented by two operations
which give the ability to manage software packages of different types on the same device.

- `software_list` is used to fetch a pertinent subset of the software packages installed on a device.
- `software_update` let the user install, update and remove software packages on a device.

## software_list MQTT API

The `software_list` operation API follows the [generic %%te%% rules for operations](device-management-api.md):
- The `te/<device-topic-id>/cmd/software_list` topic is used to publish the type of software packages
  that can be managed on the device with the given topic identifier.
- Each `te/<device-topic-id>/cmd/software_list/+` topic is dedicated to a software list command instance,
- The workflow is [generic with `"init"`, `"executing"`, `"successful"` and `"failed"` statuses](references/agent/device-management-api.md#operation-workflow).

### Operation registration

The registration message of the `software_list` operation on a device:
- must provide a `types` list of the types of software package that can be installed on this device (e.g. `["apt", "docker"]`)
- can provide a description of the operation and of each supported package type.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_list' '{
    "description": "List software packages installed on the device",
    "types": [
      "apt",
      "docker"
    ]
}'
```

### init state

A `software_list` command has nothing to provide beyond a `status` field.
This empty message stands for a request of the list of software currently installed.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_list/c8y-2023-09-25T14:34:00' '{
    "status": "init"
}'
```

### executing state

Just before starting the command execution, the agent marks the command as executing
by publishing a retained message that repeats the former command except that:

- the `status` is set to `executing`

### successful state

The payload for a successful `software_list` command has two fields:

- the `status` is set to `successful`
- a `currentSoftwareList` field is added with the *new* list of packages installed on the device
    - These packages are grouped by software package type:
       ```json
       {
          "currentSoftwareList": [
            {
              "type": "apt",
              "modules": [ "..." ]
            },
            {
              "type": "docker",
              "modules": [ "..." ]
            }
         ]
      }
      ```
    - Each installed package is given:
        - the package `"name"`,
        - the installed `"version"`.

As an example, here is a (simplified) status message for a successful `software_list` command on a child device:

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_list/c8y-2023-09-25T14:34:00' '{
    "status": "successful",
    "currentSoftwareList": [
        {
            "type": "debian",
            "modules": [
                {
                    "name": "nodered",
                    "version": "1.0.0",
                },
                {
                    "name": "collectd",
                    "version": "5.12"
                }
            ]
        },
        {
            "type": "docker",
            "modules": [
                {
                    "name": "nginx",
                    "version": "1.21.0",
                },
            ]
        }
    ]
}'
```

### failed state

The payload for a failed `software_list` is made of two fields:

- the `status` is set to `failed`
- a `reason` text field is added with the root cause of the failure

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_list/c8y-2023-09-25T14:34:00' '{
    "status": "failed",  
    "reason": "Permission denied",
}'
```

## software_update MQTT API

The `software_update` operation API follows the [generic %%te%% rules for operations](device-management-api.md):
- The `te/<device-topic-id>/cmd/software_update` topic is used to publish the type of software packages
  that can be managed on the device with the given topic identifier.
- Each `te/<device-topic-id>/cmd/software_update/+` topic is dedicated to a software update command instance,
  and is used to publish the subsequent states of the command execution.
- The workflow is [generic with `"init"`, `"executing"`, `"successful"` and `"failed"` statuses](references/agent/device-management-api.md#operation-workflow).

### Operation registration

The registration message of the `software_update` operation on a device:
- must provide a `types` list of the types of software package that can be installed on this device (e.g. `["apt", "docker"]`)
- can provide a description of the operation and of each supported package type.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_update' '{
    "description": "Install, update and remove software packages",
    "types": [
      "apt",
      "docker"
    ]
}'
```

:::info
The software types are currently not sent to the cloud (e.g. Cumulocity IoT), however this is planned in a future release.
:::

### init state

A `software_update` command is defined by an `"updateList"` array giving the packages to install, update or remove.

- The `"updateList"` field is a list of software update actions.
  - These actions are grouped by software package type:
     ```json
     {"updateList": [
        {
            "type": "apt",
            "modules": [ "..." ]
        },
        {
            "type": "docker",
            "modules": [ "..." ]
        }
    ]}
     ```
   - Each action is either to `"install"` or `"remove"` a software package.
      - `"action": "install"` is to be understood as a goal:
        "this package must be installed with this version" whatever the actual action to reach this goal,
        being a fresh install, an upgrade or a downgrade.
      - `"action": "remove"` is also to be understood as a goal: "that package must not be installed".
   - An action provides:
      - the package `"name"` (as known by the package packager),
      - optionally a `"version"` (using the same conventions as the package manager),
      - optionally an `"url"` from where to download the package.

As an example, here is a message requesting a `software_update` on a child device:

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_update/c8y-2023-09-25T14:53:00' '{
    "status": "init",
    "updateList": [
        {
            "type": "apt",
            "modules": [
                {
                    "name": "nodered",
                    "version": "1.0.0",
                    "action": "install"
                },
                {
                    "name": "collectd",
                    "version": "5.12",
                    "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                    "action": "install"
                }
            ]
        },
        {
            "type": "docker",
            "modules": [
                {
                    "name": "nginx",
                    "version": "1.21.0",
                    "action": "install"
                },
                {
                    "name": "mongodb",
                    "version": "4.4.6",
                    "action": "remove"
                }
            ]
        }
    ]
}'
```

### executing state

Just before starting the command execution, the agent marks the command as executing
by publishing a retained message that repeats the former command except that:

- the `status` is set to `executing`

### successful state

The payload for a successful `software_update` command
repeats the same content as the former request except that:

- the `status` is set to `successful`.

As an example, here is a status message for a successful `software_update` command on a child device:

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_update/c8y-2023-09-25T14:53:00' '{
    "status": "successful",  
    "updateList": [
        {
            "type": "apt",
            "modules": [
                {
                    "name": "nodered",
                    "version": "1.0.0",
                    "action": "install"
                },
                {
                    "name": "collectd",
                    "version": "5.12",
                    "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                    "action": "install"
                }
            ]
        },
        {
            "type": "docker",
            "modules": [
                {
                    "name": "nginx",
                    "version": "1.21.0",
                    "action": "install"
                },
                {
                    "name": "mongodb",
                    "version": "4.4.6",
                    "action": "remove"
                }
            ]
        }
    ]
}'
```

### failed state

The payload for a failed `software_update` command
repeats the same content as the former request except that:

- the `status` is set to `failed`
- a `reason` text field is added with the root cause of the failure
- a `failures` array field might be added to list the errors for all the failing actions.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/child001///cmd/software_update/c8y-2023-09-25T14:53:00' '{
    "status": "failed",  
    "reason": "Partial failure: Could not install collectd and nginx",
    "updateList": [
        {
            "type": "apt",
            "modules": [
                {
                    "name": "nodered",
                    "version": "1.0.0",
                    "action": "install"
                },
                {
                    "name": "collectd",
                    "version": "5.12",
                    "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                    "action": "install"
                }
            ]
        },
        {
            "type": "docker",
            "modules": [
                {
                    "name": "nginx",
                    "version": "1.21.0",
                    "action": "install"
                },
                {
                    "name": "mongodb",
                    "version": "4.4.6",
                    "action": "remove"
                }
            ]
        }
    ],
    "failures":[
        {
            "type":"apt",
            "modules": [
                {
                    "name":"collectd",
                    "version":"5.7",
                    "action":"install",
                    "reason":"Network timeout"
                }
            ]
        },
        {
            "type":"docker",
            "modules": [
                {
                    "name": "mongodb",
                    "version": "4.4.6",
                    "action":"remove",
                    "reason":"Other components dependent on it"
                }
            ]
        }
    ]
}'
```

## tedge-agent implementation

### Software management plugins

The `tedge-agent` service uses software management plugins to interact with the actual package managers.

For each type of software package supported on the device must be provided a specific software management plugin:

- A plugin is an executable file implementing the [software plugin API](../software-management-plugin-api.md),
  to `list`, `install` and `remove` software packages of a specific type.
- These plugins are looked up by `tedge-agent` in the plugin directory (`/etc/tedge/sm-plugins` if not specified otherwise).
- `tedge-agent` uses the file name of a plugin executables as the software package type name.

### Settings

`tedge-agent` behavior on `software_update` commands can be configured with `tedge config`.

- `software.plugin.default` set the default software plugin to be used for software management on the device. 
- `software.plugin.max_packages` set the maximum number of software packages reported for each type of software package.

## Custom implementation

%%te%% users can implement their own support for software management to address the specificities of their devices.
- This can be done leveraging the `tedge-agent` and implementing a custom [software plugin](../software-management-plugin-api.md).
- If for some reasons the `tedge-agent` cannot run on the target hardware,
  then a service must be implemented to support the `software_list` and `software_update` operation, as described below.
  In this case, the service is free to choose its own mechanisms to manage software packages
  and can even run on a device that is not the target hardware.



