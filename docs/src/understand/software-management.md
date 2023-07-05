---
title: Software Management
tags: [Concept, Software Management]
sidebar_position: 7
---

# Software Management with thin-edge.io

With thin-edge.io you can ease the burden of managing packages on your device.
Software Management operates end to end from a cloud down to the OS of your device and reports statuses accordingly.

## Software management components

Software Management uses the following 3 components to perform software operations:
**Cloud Mapper**, **Agent**, and **Software Management Plugin**.

You can find the diagrams which explain how those 3 components interact with each other from [Software Management Agent Specification](https://thin-edge.github.io/thin-edge.io-specs/software-management/sm-agent.html).

### Cloud Mapper

The **Cloud Mapper** converts from/to cloud-specific format to/from cloud-agnostic format.
It communicates with the dedicated IoT cloud platform and the **Tedge Agent**.

### Tedge Agent

The **Tedge Agent** addresses cloud-agnostic software management operations e.g. listing current installed software list, software update, software removal.
Also, the Tedge Agent calls an **SM Plugin(s)** to execute an action defined by a received operation.

The key points are that the **Tedge Agent** is always generic in cloud platforms and software types, and **Cloud Mapper** handles cloud-specific actions.

### Software Management Plugin

The **Software Management Plugin** is dedicated to defining the behaviour of software actions (list, update, remove) per software type (apt, docker, etc.)

## Related documents
1. [How to install and enable software management?](../operate/installation/install_and_enable_software_management.md)
2. [Manage my device software](../start/software-management.md)
3. [Write my software management plugin](../extend/write-my-software-management-plugin.md)
4. [The Software Management Plugin API](../references/plugin-api.md)
5. [Software Management Specification](https://github.com/thin-edge/thin-edge.io-specs/tree/main/src/software-management)
