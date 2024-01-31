---
title: Overview
slug: /
sidebar_position: 0
---

Welcome to %%te%%'s documentation!

%%te%% is an open-source development toolbox designed for rapid development of IoT agents.
It is based on a versatile set of ready-to-use software components
that can be easily combined with application-specific extensions
into smart, secure, robust and efficient IoT agents
which integrate cloud services, edge computing and operational technologies.

A typical agent uses as a foundation the building blocks provided by %%te%%
for telemetry data processing and cloud connectivity as well as for device monitoring, configuration and updates.
In combination with these blocks, the agent designer can provide application-specific extensions,
which cooperate with %%te%% over MQTT and HTTP along a JSON API,
to address any hardware, protocol or use-case specificity.

As such, %%te%% is good choice for implementing smart-equipment
that collect in-situ real-time data, perform analytics on the edge, forward key data to the cloud,  
and need to be secured, configured and updated at scale.

## How to start {#start}

The easiest way to get started is either to install the docker based [demo container](https://github.com/thin-edge/tedge-demo-container)
that showcases %%te%% and all its features or with the [beginner-friendly tutorial](start/getting-started.md)
that introduces %%te%% and guides you on how to install it on a Raspberry Pi.
After the installation you can directly connect your device to [Cumulocity IoT](https://www.cumulocity.com/guides/concepts/introduction/),
and then monitor it from the cloud.

You can also explore the main use-cases using these [tutorials](start/index.md).
You will learn to:

- [install %%te%% on your specific hardware](install/index.md),
- connect your device to your cloud, whether [Cumulocity IoT](start/connect-c8y.md),
  [Azure IoT](start/connect-azure.md) or [AWS IoT](start/connect-aws.md),
- [send telemetry data](start//send-measurements.md), [alarms](start//raise-alarm.md) and [events](start//send-events.md),
- operate, configure, update, monitor your device.


## The concepts {#concepts}

Better understand how %%te%% works by reviewing the core [Concepts](understand/index.md).

## How to operate a device {#operate}

%%te%% provides a set of building blocks to operate, configure, update, monitor your devices.

* Use the [how-to guides](operate/index.md) on a daily basis
* Refer to the [reference guides](references/index.md) for any in-depth details

## How to extend {#extend}

One of the core feature of %%te%% is to be extensible.

- [Write a software-management plugin](extend/software-management.md)
- [Build Operating System images with %%te%% setup to perform Over-the-Air (OTA) updates](extend/firmware-management/index.md)

## How to contribute {#contribute}

[%%te%%](https://github.com/thin-edge/thin-edge.io) is an open-source project
released under the [Apache License - Version 2.0](https://github.com/thin-edge/thin-edge.io/blob/main/LICENSE.txt).

All contributions are greatly appreciated.
It can be by reporting issues, improving this documentation, adding new extensions or contributing to the main code base.

Please refer to the [contribution guide](https://github.com/thin-edge/thin-edge.io/blob/main/CONTRIBUTING.md)
and the [contributor documentation](contribute/index.md).
