---
title: Auto-registration
tags: [Child-Device, Registration]
sidebar_position: 1
description: Auto-register child devices and services over MQTT
---

# Auto Registration

%%te%% provides an auto-registration mechanism for clients that do not want to do an explicit registration.
To leverage this, they must conform to the **default topic scheme** that clearly demarcates the `device` and `service` as follows:

```
te/device/<device_id>/service/<service_id>
```

With auto-registration, when a measurement message is published to `te/device/rpi1001///m/cpu_usage` topic
without a prior explicit registration of that entity,
a `child-device` entity: `device/rpi1001//` is auto-registered as an **immediate child device** of the main device
before the `cpu_usage` measurement is associated to it.

Similarly, publishing to `te/device/rpi1002/service/collectd/m/cpu_usage` would result in the auto-registration of
both the `child-device` entity: `device/rpi1002//` and its `service` entity: `device/rpi1002/service/collectd`.

The auto-registered entities are persisted with the local MQTT broker by the `tedge-agent` with their auto-generated registration messages.
For example, the child device `device/rpi1002//` is registered as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/rpi1002//' '{
  "@type": "child-device",
  "@parent":"device/main//",
  "name": "rpi1002",
}'
```

Similarly, the service: `device/rpi1002/service/collectd` is registered as follows:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/rpi1002/service/collectd' '{
  "@type": "service",
  "@parent":"device/rpi1002//",
  "name": "collectd",
}'
```

:::note
The %%te%% main device is pre-registered with the reserved default topic identifier: `device/main//`.
:::

## Configuration

The auto-registration feature is turned on by default, ideal for simple fleets with only a single level of child devices,
as auto-registered child devices are always linked to the main device as its immediate children.
If your device fleet has a nested hierarchy of child devices spanning multiple levels, then auto-registration must be turned off,
to prevent all child devices(even the nested ones) wrongly getting auto-registered as immediate children of the main device.

Auto-registration can be turned off with a configuration setting as follows:

```sh
tedge config set agent.entity_store.auto_register false
```

After this configuration change, the `tedge-agent` must be restarted for it to take effect.

Turning off auto-registration can be beneficial in certain other scenarios as well:
- If the default topic scheme does not match your fleet, and you'd like to use your own [custom topic scheme](../../contribute/design/mqtt-topic-design.md#using-custom-identifier-schemas) for your entities (e.g: `factory1/floor1/painting/robot1`)
- You still want to use the default topic scheme, but need more control over the metadata of the entities that are registered,
  like registering every entity with an explicit `@id` or having the entities registered with some initial twin data.

:::caution
Combining auto-registration with explicit registration using the MQTT API is highly discouraged,
as it can lead to unexpected results like nested child devices getting auto-registered as immediate child devices,
or services getting registered under the wrong parent device.
If explicit registration is required in a deployment where auto-registration is already enabled,
using the HTTP registration API is recommended, before any telemetry data is published by those clients.
:::
