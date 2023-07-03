---
slug: /
sidebar_position: 0
---

# Overview

Welcome to `thin-edge.io`'s documentation!

`thin-edge.io` is an open-source development toolbox designed for rapid development of IoT agents.
It is based on a versatile set of ready-to-use software components
that can be easily combined with application-specific extensions
into smart, secure, robust and efficient IoT agents
which integrate cloud services, edge computing and operational technologies.

A typical agent uses as a foundation the building blocks provided by thin-edge
for telemetry data processing and cloud connectivity as well as for device monitoring, configuration and updates.
In combination with these blocks, the agent designer can provide application-specific extensions,
which cooperate with thin-edge over MQTT and HTTP along a JSON API,
to address any hardware, protocol or use-case specificity.

As such, `thin-edge.io` is good choice for implementing smart-equipment
that collect in-situ real-time data, perform analytics on the edge, forward key data to the cloud,  
and need to be secured, configured and updated at scale.

## How to start

The easiest way to get started is with the [beginner-friendly tutorial](start/getting-started.md)
that introduces `thin-edge.io` and guides you on how to install it on a Raspberry Pi,
connect your device to [Cumulocity IoT](https://www.cumulocity.com/guides/concepts/introduction/),
and then monitor it from the cloud.

You can also explore the main use-cases using these [tutorials](start/index.md).
You will learn to:

- [install thin-edge on your specific hardware](install/index.md),
- connect your device to your cloud, whether [Cumulocity IoT](start/connect-c8y.md),
  [Azure IoT](start/connect-azure.md) or [AWS IoT](start/connect-aws.md),
- [send telemetry data](start//send-thin-edge-data.md), [alarms](start//raise-alarm.md) and [events](start//send-events.md),
- operate, configure, update, monitor your device.

## The concepts

  - [Architecture FAQ](understand/faq.md)
  - [Thin Edge Json](understand/thin-edge-json.md)
  - [The Mapper](understand/tedge-mapper.md)
  - [Software Management](understand/software-management.md)

## How to operate a device with thin-edge.io

Thin-edge provides a set of building blocks to operate, configure, update, monitor your devices.

* Use the [how-to guides](operate/index.md) on a daily basis
* Refer to the [reference guides](references/index.md) for any in-depth details

## How to extend thin-edge.io

One of the core feature of thin-edge is to be extensible.

- [Write a software-management plugin](extend/write-my-software-management-plugin.md)
- [Build Thin Edge for a Yocto Linux distribution](extend/yocto-linux.md)

## How to contribute

[`thin-edge.io`](https://github.com/thin-edge/thin-edge.io) is an open-source project
released under the [Apache License - Version 2.0](https://github.com/thin-edge/thin-edge.io/blob/main/LICENSE.txt).

All contributions are greatly appreciated.
It can be by reporting issues, improving this documentation, adding new extensions or contributing to the main code base.

Please refer to the [contribution guide](https://github.com/thin-edge/thin-edge.io/blob/main/CONTRIBUTING.md)
and the [contributor documentation](contribute/index.md).