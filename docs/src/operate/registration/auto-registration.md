---
title: Entity MQTT APIs
tags: [Child-Device, Registration]
sidebar_position: 1
description: Register child devices and services with MQTT
---

# Auto Registration

%%te%% provides an auto-registration mechanism for clients that do not want to do an explicit registration,
as long as they are immediate child devices of the main device or services linked to a device.
To leverage this, they must conform to the **default topic scheme** that clearly demarcates the `device` and `service` as follows:

```
te/device/<device_id>/service/<service_id>
```

With auto-registration, when a measurement message is published to `te/device/rpi1001///m/cpu_usage` topic
without a prior explicit registration of that entity,
a `child-device` entity: `device/rpi1001//` is auto-registered as an immediate child device of the main device
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

The auto-registration feature is turned on by default and can be turned off using the following configuration setting:

```sh
tedge config set agent.entity_store.auto_register false
```

After this configuration change, the `tedge-agent` must be restarted for it to take effect.

:::caution
Combining auto-registration with explicit registration using the MQTT API is highly discouraged,
as it can lead to unexpected results like nested child devices getting auto-registered as immediate child devices,
or services getting registered under the wrong parent device.
If explicit registration is required in a deployment where auto-registration is already enabled,
using the HTTP registration API is recommended, before any telemetry data is published by those clients.
:::
