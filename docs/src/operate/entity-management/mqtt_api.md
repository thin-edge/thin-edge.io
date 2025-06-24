---
title: MQTT API
tags: [Child-Device, Registration]
sidebar_position: 1
description: Register child devices and services with MQTT
---

# MQTT API for Entity Management

%%te%% provides an MQTT API to manage all the entities (devices and services) attached to a main device.
These interfaces let you create, update and delete entities as well as observe changes.

When compared to the HTTP API, the MQTT API excels at notifying subscribers in real-time about entity-related changes
such as creation, updates, or deletion of child devices and services.
However, unlike the HTTP API, the MQTT API doesn't provide immediate feedback on whether an operation succeeded or failed,
making it less suitable for scenarios where confirmation is crucial.
For example, when an entity registration is attempted by publishing the metadata payload,
there is no feedback on whether the registration succeeded or not.

## Create a new entity {#create-entity}

A new entity can be registered by publishing the entity definition to the MQTT topic that contains the **entity topic identifier**.
The payload must contain at least the `@type` of the entity: whether it is a `child-device` or `service`.

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

Any additional fields included in the payload are considered as initial [twin data](../../references/mqtt-api.md#twin-metadata) for that entity,
and they are re-published to the corresponding twin topics.

### Example: Create a new child device

Register as a child device with the name "child0". Assign the child device to the main device (`device/main//`) and give it the topic-id of `device/child0//` which will be used to reference it in all other API calls (both for REST and MQTT).

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child0//' '{
  "@type": "child-device"
}'
```

### Example: Create a service under a child device

Register `device/child0/service/nodered` as a service of `device/child0//`:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child0/service/nodered' '{
  "@type": "service"
}'
```

When the topic id of the service follows the default topic scheme (`te/device/<device_id>/service/<service_id>`),
the parent of the service is derived from the topic itself (`device/child0//` in this case).
If a custom topic scheme is used, then the parent is assumed to be `device/main//` by default, when not specified.

### Example: Create a nested child device

Register `device/child01//` as a child device of `device/child0//` which is specified as the `@parent`:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device",
  "@parent": "device/child0",
}'
```

When the `@parent` is not specified for a child device, it is assumed to be the main device.

### Example: Create a child device with an external id

Register `device/child01//` as a child device of the main device with a unique id: `XYZ001:child01`:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device",
  "@id": "XYZ001:child01"
}'
```

This `@id` is used to uniquely identify an entity from others.
For example, the cloud mappers use this unique id to register these entities with the cloud.
When an `@id` is not provided, the mappers are free to compute a unique id from their topic id, which is also unique.

### Example: Create a child device with initial twin data

Register `device/child01//` with initial twin values: `name` and `type`.

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device",
  "name": "Child 01",
  "type": "Raspberry Pi 4"
}'
```

Unlike the reserved keys like `@type`, %%te%% does not interpret these twin keys in any special way.
For example, the `type` value here captures the type of the device and has nothing to do with the entity `@type`.
They are just re-published to their corresponding twin topics (`twin/name` and `twin/type`) as-is.
So, the values can be any valid twin values.

:::warning
Using the `@` prefix for such twin data keys is discouraged as this prefix is reserved by %%te%%.
:::

## Get an entity {#get-entity}

Get an entity's metadata (e.g. default name and type, and parent).

### Example: Get a child device

Get the child device `device/child0//`:

```sh te2mqtt formats=v1
tedge mqtt sub 'te/device/child0//' --count 1
```

:::note
The `--count` value of `1` is used to exit the `tedge mqtt sub` program once the metadata message is received.
:::

### Example: Get all devices/child-devices/services

Fetch the metadata of all registered entities:

```sh te2mqtt formats=v1
tedge mqtt sub 'te/+/+/+/+' --duration 1s
```

:::note
The `--duration` value of `1s` is used to exit the `tedge mqtt sub` program once all the entity metadata messages are received.
If the number of entities are too high, adjust this timeout accordingly to ensure that all messages are received.
:::

## Update an entity {#update-entity}

An entity definition can be updated by publishing the new entity definition, replacing the existing one.
Updates are limited to the `@parent` and `@health` properties only,
so other properties like `@type` and `@id` cannot be updated after the registration.

:::note
The complete definition of the new entity must be provided in the payload
unlike the [HTTP PATCH API](./rest_api.md#update-entity), that accepts the specific fragments to be updated.
:::note

### Example: Update the parent of an entity

Update the parent of the entity `device/child01//` by making it a child of `device/child0//`:

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' '{
  "@type": "child-device",
  "@parent": "device/child0",
}'
```

## Delete an entity {#delete-entity}

An entity and its descendants (immediate children and nested children) can be deregistered by publishing
an empty retained messages to the MQTT topic corresponding to its entity topic identifier.

### Example: Delete a child device

Remove a child device and any of its children.

```sh te2mqtt formats=v1
tedge mqtt pub -r 'te/device/child01//' ''
```

:::note
The deregistration of descendant entities is done asynchronously by the `tedge-agent`
and hence the completion time would vary based on how deep the hierarchy is.
Using the [HTTP API](./rest_api.md#delete-entity) is recommended for deregistration as it provides clear feedback on completion.
:::
