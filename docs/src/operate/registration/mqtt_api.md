---
title: Entity management REST APIs
tags: [Child-Device, Registration]
sidebar_position: 1
description: Register child devices and services with %%te%% MQTT APIs
---

# MQTT APIs for Entity Management

The entity management MQTT APIs of %%te%% allows MQTT clients to register and deregister entities (child devices and services).

## Register entity

A new entity can be registered by publishing the entity definition to the MQTT topic that contains the **entity topic identifier**.
The payload must contain at least the `@type` of the entity: whether it is a `child-device` or `service`.

**Example: Register a child device**

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device"
}'
```

**Example: Register a service of a device**

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/main/service/nodered' '{
  "@type": "service"
}'
```

:::note
The supported `@type` values are `device`, `child-device` and `service`.
The `device` type is reserved for the main %%te%% device which is pre-registered when it is bootstrapped.
:::

Other supported (optional) fields in the registration payload include:

- `@parent`: Topic ID of the parent entity.
  Required for nested child devices or services where the parent cannot be derived from the topic.
- `@id`: External ID for the entity.
- `@health`: Topic ID of the health endpoint service of this entity.
  Valid only for `child-device` entities.
  By default, it is the `tedge-agent` service on that device.

**Example: Register a nested child device**

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device",
  "@parent": "device/child0",
  "@id": "XYZ001:child01"
}'
```

**Example: Register a child device with an external id**

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device",
  "@id": "XYZ001:child01"
}'
```

Any additional fields included in the payload are considered as initial twin data for that entity,
and they are re-published to the corresponding twin topics.

**Example: Register a child device with initial twin data**

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device",
  "name": "Child 01",
  "type": "Raspberry Pi 4"
}'
```

:::warning
Using the `@` prefix for such twin data keys is discouraged as this prefix is reserved by %%te%%.
:::

### Auto Registration

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

The auto-registered entities are persisted with the MQTT broker by the `tedge-agent` with their auto-generated registration messages.
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

## Deregister entity

An entity and its descendants (immediate children and nested children) can be deregistered by publishing
an empty retained messages to the MQTT topic corresponding to its entity topic identifier.

**Example**

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' ''
```

:::note
The deregistration of descendant entities is done asynchronously by the `tedge-agent`
and hence the completion time would vary based on how deep the hierarchy is.
Using the [HTTP API](./http_api.md#delete-entity) is recommended for deregistration as it provides clear feedback on completion.
:::