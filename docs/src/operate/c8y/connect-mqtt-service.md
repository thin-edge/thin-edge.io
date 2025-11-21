---
title: 🚧 Connecting to Cumulocity MQTT Service
tags: [Operate, Cloud, Connection, Cumulocity]
description: Connecting %%te%% to Cumulocity
sidebar_position: 1
unlisted: true
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';
import BrowserWindow from '@site/src/components/BrowserWindow';

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing relevant settings which will be reflected in the instructions. It makes it even easier to explore and use %%te%%.

<UserContextForm settings="DEVICE_ID,C8Y_URL" />

The user context will be persisted in your web browser's local storage.
:::

## MQTT Service

%%te%%, when connected to Cumulocity, connects to its [Core MQTT](https://cumulocity.com/docs/device-integration/mqtt/) 
endpoint by default, which only supports a predefined set of topics that the device can publish data to
and receive data from in predefined data formats (e.g: SmartREST or JSON over MQTT).
The [Cumulocity MQTT service](https://cumulocity.com/docs/device-integration/mqtt-service) on the other hand,
is the next-gen MQTT endpoint offered by Cumulocity,
which allows devices to publish and receive data using user-defined custom topic and payload formats as well.

More information about the two MQTT interfaces offered by Cumulocity in the following table.

|Name|Port|Description|Status|
|----|----|-----------|------|
|Core MQTT|8883|Allows devices to send messages directly into Cumulocity, provided that the device implements the pre-defined topic schema and payload formats of Core MQTT|[General Availability](https://cumulocity.com/docs/2024/glossary/g/#ga)|
|MQTT Service|9883|Allows devices to send and receive arbitrary payloads on any MQTT topic|[Public Preview](https://cumulocity.com/docs/2024/glossary/p/#public-preview) (subject to change)|

:::caution
The Cumulocity MQTT service is still in Public Preview, and as such, the interface is subject to change.
It is strongly advised to avoid using it in production scenarios until the interface has stabilized.
If you do decide to use it in production systems, then expect to have to do run some migration activities
to update the %%te%% version and modify interfaces, configuration etc once the feature goes into General Availability.
:::

### Configure the device

:::note
The examples in this section demonstrate a device using a custom topic scheme to
publish temperature measurements to the `sensors/temperature/measurement` topic in the cloud and
receive commands to adjust the sampling interval from the cloud on the `sensors/temperature/set-config` topic.
:::

Most of the configuration used to connect to the Cumulocity MQTT service endpoint are the same as
the ones used to connect to the Cumulocity Core MQTT endpoint.

1. Configure Cumulocity URL, if not already set.

   <UserContext>
   
   ```sh
   sudo tedge config set c8y.url $C8Y_URL
   ```
   
   </UserContext>

   :::note
   Though the `c8y.url` config is set in this step, the `c8y.mqtt` config is used under-the-hood for the connection,
   as this config is derived from `c8y.url`, by default.

   If the MQTT service url is different from the one that would be derived from `c8y.url`,
   then set`c8y.mqtt` explicitly.
   :::

1. Enable connection to MQTT service endpoint

   ```sh
   sudo tedge config set c8y.mqtt_service.enabled true
   ```

   :::note
   The `c8y.mqtt` value is derived differently based on whether mqtt service is enabled or not.
   For example, when `c8y.url` is `example.cumulocity.com` and when `mqtt_service` is enabled,
   `c8y.mqtt` would be derived as `example.cumulocity.com:9883` (the default mqtt service endpoint).
   else it would be `example.cumulocity.com:8883` (the default core mqtt endpoint).
   :::


1. Provide the topics to subscribe to (e.g: topic to receive sensor config updates)

   ```sh
   sudo tedge config set c8y.mqtt_service.topics sensors/temperature/set-config
   ```

1. Make Cumulocity trust the device certificate as described [here](./connect.md#making-the-cloud-trust-the-device),
   if not already done.

1. Connect the device

   ```sh
   sudo tedge connect c8y
   ```

   This step establishes the bridge connection to the mqtt service endpoint instead of the core mqtt endpoint.
   All MQTT traffic using both the built-in topics (e.g: SmartREST) as well as the user-provided custom topics
   are routed to the MQTT service endpoint, completely bypassing the core MQTT endpoint.

   :::note
   If the device was previously connected to Cumulocity (the Core MQTT endpoint),
   doing a `sudo tedge reconnect c8y` after steps 2 and 3 would have sufficed.
   :::

1. Once connected, all messages published to `c8y/mqtt/out/#` topics are forwarded to the MQTT service endpoint,
   without the `c8y/mqtt/out/` prefix.

   For example, to publish the temperature measurement:

   ```sh
   tedge mqtt pub c8y/mqtt/out/sensors/temperature/measurement 25
   ```

   The message will be published to the `sensors/temperature/measurement` topic on the MQTT service,
   and its receipt can be validated on Cumulocity.
2. Similarly, any messages published to the subscribed `sensors/temperature/set-config` topic on Cumulocity
   are published to the corresponding local bridge topic with a `c8y/mqtt/in/` topic prefix.

   To see the set configuration commands received from the cloud:

   ```sh
   tedge mqtt sub c8y/mqtt/in/sensors/temperature/set-config
   ```
