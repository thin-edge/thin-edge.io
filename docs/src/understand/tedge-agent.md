---
title: The Agent
tags: [Concept, MQTT]
sidebar_position: 5
---

# Thin-edge Agent

Thin-edge agent is a set of services that implement the device management operations on a device.
Examples of such device management functionality include:
- software management, checking installed packages and updating these packages
- configuration management, checking and updating configuration files
- firmware management, updating the firmware of the device
- device restart
- remote access to a console on the device from the cloud
- log file management, retrieving excerpts from log files

:::note
In the current version of thin-edge, the agent features are not implemented by a single executable,
but by a set of executables:

- `tedge-agent`
- `c8y-configuration-plugin`
- `c8y-firmware-plugin`
- `c8y-log-plugin`
- `c8y-remote-access-plugin`

The short-term plan is to re-organize these plugins to move the Cumulocity aspects into the Cumulocity mapper
and to group the management operations into a single executable. 
:::

Thin-edge agent acts as a device connector:
- listening to operation requests published on the MQTT bus
- delegating the actual operations to the operating system or other components
- reporting progress of the requests

## Operation MQTT topics

Operation requests are published by the requesters on operation specific topics:

```text
tedge/commands/req/{operation-type}/{operation-action}
```

Where the combination of `operation-type` and `operation-action` is the well-known name of the operation request, such as:
* `software/update`
* `control/restart`

The corresponding operation responses are published to associated topics:

```text
tedge/commands/res/${operation-type}/${operation-action}
```

Here are the topics used by the device management operations

| Operation          | Request Topic                         | Response Topic                         |
| ------------------ |---------------------------------------|----------------------------------------|
| Get Software List  | `tedge/commands/req/software/list`    | `tedge/commands/res/software/list`     |
| Software Update    | `tedge/commands/req/software/update`  | `tedge/commands/res/software/update`   |
| Get Configuration  | `tedge/commands/req/config_snapshot`  | `tedge/commands/res/config_snapshot`   |
| Set Configuration  | `tedge/commands/req/config_update`    | `tedge/commands/res/config_update`     |
| Get Log            | `tedge/commands/req/log/get`          | `tedge/commands/res/log/get`           |
| Restart  device    | `tedge/commands/req/control/restart`  | `tedge/commands/res/control/restart`   |
| Remote  connect    | `tedge/commands/req/control/connect`  | `tedge/commands/res/control/connect`   |

