---
title: Running the Agent
tags: [Reference, Agent]
sidebar_position: 1
description: Running the agent for device management on main and child devices
---

In order to enable %%te%% device management features on a device,
being the main device or a child device,
one has to install and run the `tedge-agent` service on this device.

## On the main device

Per default, `tedge-agent` assumes it run on the main device.

```sh title="running tedge-agent on the main device"
tedge-agent
```

## On a child-device

To launch `tedge-agent` on a child device,
one has to configure the [topic identifier](../mqtt-api.md#group-identifier)
on this device to point to the appropriate topic identifier.

```sh title="running tedge-agent on the child device child-007"
sudo tedge config set mqtt.device_topic_id device/child-007//
tedge-agent 
```

The configured device topic identifier can also be overridden on the command line.

```sh title="running tedge-agent on the child device child-007"
tedge-agent --mqtt-device-topic-id device/child-007//
```

## Using a custom identifier schema

If using a [custom identifier schema](/contribute/design/mqtt-topic-design.md#using-custom-identifier-schemas),
then the device topic identifier has to be configured even for the main device.

```sh title="running tedge-agent when using a custom identifier schema"
sudo tedge config set mqtt.topic_root acme
sudo tedge config set mqtt.device_topic_id factory01/hallA/packaging/belt001
tedge-agent 
```

Or, using the command line:
```sh title="running tedge-agent while using a custom identifier schema"
tedge-agent --mqtt-topic-root acme --mqtt-device-topic-id factory01/hallA/packaging/belt001
```
