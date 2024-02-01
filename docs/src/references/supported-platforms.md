---
title: Supported Platforms
tags: [Reference, Installation]
sidebar_position: 15
description: List of supported platforms, CPU architectures and resource usage
---

%%te%% can be run on any Linux based Operating System, such as; Debian/Ubuntu, Alpine, RHEL, Fedora, Pokey (Yocto) etc.

In addition to a Linux based Operation system, the following requirements must also be fulfilled:

* At least ~40MB of RAM on the gateway device, ~8MB on a child device (see [notes](#memory-usage))
* [mosquitto](https://github.com/eclipse/mosquitto) MQTT Broker (see [notes](#recommended-mosquitto-version))

:::tip
If you are looking for new hardware it is highly recommended to choose a CPU which has at least 2 cores, as it provides a much more responsive experience and should provide enough processing headroom for future requirements.
:::

:::note
mosquitto will be installed via the installation script, however it needs to be available from a software repository, however mosquitto is generally available in most popular Linux based operating systems.
:::

## Recommended mosquitto version

Whilst %%te%% will work with any mosquitto version, it is recommended to use mosquitto version &gt;=2.0.18 as it includes some important fixes regarding the MQTT bridge functionality.

If you can't use the recommended version, then be aware of the following issues which affect various mosquitto versions in some corner cases:

|Mosquitto Version|Issues|
|-----------------|------|
|2.0.11|https://github.com/eclipse/mosquitto/issues/2604|
|2.0.15|https://github.com/eclipse/mosquitto/issues/2634|

## Supported CPU Architectures

%%te%% is developed in Rust which is a compiled language and each executable is specific for different CPU Architectures (e.g. x86_64/amd64, aarch64/arm64, armv7, armv6 etc.).  


The following sections give detailed information about which CPU Architectures which are currently supported.

### Supported x86_64 / amd64 Processors

x86_64 is a 64 bit CPU architecture used by the main stream processors of Intel and AMD.

### Supported ARM Processors

The following are the supported ARM processors:

* ARM11 (aka ARMv6)
* ARM Cortex-A Family (e.g. ARMv7-A, ARMv8.2-A, ARMv9.2-A)

Generally the newer ARM processors should be supported, as they are mostly based on the arm64/aarch64 architecture, however please let us know if something does not work as expected.

## Memory Usage

The exact memory usage of %%te%% and the MQTT broker depends on a many factors; such factors are listed below:

* Which components are running?
* How many messages are being sent to the MQTT broker?
* How long should mosquitto buffer messages during connectivity outages?

Despite the above factors which influence the memory usage, a guideline of the typical memory usage for each %%te%% components is shown below, where the memory usage refers to the typical [Resident Set Size (RSS)](https://en.wikipedia.org/wiki/Resident_set_size).

|Name|Typical Memory Usage (MiB)|
|--|--|
|tedge-mapper (per instance)|8|
|tedge-agent|8|
|MQTT broker (mosquitto)|10|

### Memory Usage by Scenario

To provide a better insight into the actual memory usage, the following sections contain specific scenarios, and the typical memory usage required in each scenario.

#### Scenario: Gateway device

A typical gateway device setup is a device where the MQTT broker and %%te%% are running on the single device to provide management of the gateway itself and provide cloud connectivity to other devices in the local network.

In this scenario, all of the %%te%% components are running on the gateway device. The breakdown of the typical memory usage is shown in the table below:

|Name|Typical Memory Usage (MiB)|
|--|--|
|tedge-mapper c8y (Cumulocity)|8|
|tedge-mapper collectd |8|
|tedge-agent|8|
|mosquitto|10|
|**Total**|34|

#### Scenario: Child device (connected to a local gateway)

The %%te%% component, **tedge-agent**, can also be run on a child device where it connects to an MQTT broker running on a gateway device within the same local network. In this scenario the memory requirements are considerable less as only the **tedge-agent** needs to run on the device, as the cloud connection is managed on the gateway device.

Below shows the typical memory usage breakdown in this scenario:

|Name|Typical Memory Usage (MiB)|
|--|--|
|tedge-agent|8|
|**Total**|8|
