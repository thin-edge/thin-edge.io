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

:::caution
The Cumulocity MQTT service is still in Public Preview, and as such, the interface is subject to change.
It is strongly advised to avoid using it in production scenarios until the interface has stabilized. If you do decide
to use it in production systems, then expect to have to do run some migration activities to update the %%te%% version
and modify interfaces, configuration, and update the %%te%% version, once the feature goes into General Availability.
:::

The [Cumulocity MQTT service](https://cumulocity.com/docs/device-integration/mqtt-service) is the next-gen MQTT broker offered by Cumulocity. In contrast to the existing [Cumulocity Core MQTT](https://cumulocity.com/docs/device-integration/mqtt/), the [Cumulocity MQTT service](https://cumulocity.com/docs/device-integration/mqtt-service) allows devices to publish and receive data using user-defined topics and payloads. The feature is currently in [Public Preview](https://cumulocity.com/docs/2024/glossary/p/#public-preview) which means that the interface is subject to change and some of the functionality is not yet implemented.

%%te%% currently uses [Cumulocity's Core MQTT](https://cumulocity.com/docs/device-integration/mqtt/) to establish a connection to the cloud, however it is planned that once the
MQTT Service reaches the [General Availability](https://cumulocity.com/docs/2024/glossary/g/#ga) status, it will support both the required subset of functionality currently provided
by the Core MQTT (e.g. SmartREST and JSON over MQTT communication) in addition to the user-defined topics and payloads. However, until this functionality is provided by the new service,
if you wish to use Cumulocity MQTT Service, %%te%% will have to maintain two active MQTT connections, one to the Core MQTT, and one to the MQTT Service.

More information about the two MQTT interfaces offered by Cumulocity in the following table.

|Name|Port|Description|Status|
|----|----|-----------|------|
|[Cumulocity Core MQTT](https://cumulocity.com/docs/device-integration/mqtt/)|8883|Allows devices to send messages directly into Cumulocity, provided that the device implements the pre-defined topic schema and payload formats of Core MQTT|[General Availability](https://cumulocity.com/docs/2024/glossary/g/#ga)|
|[Cumulocity MQTT Service](https://cumulocity.com/docs/device-integration/mqtt-service)|9883|Allows devices to send and receive arbitrary payloads on any MQTT topic|[Public Preview](https://cumulocity.com/docs/2024/glossary/p/#public-preview) (subject to change)|


### Configure the device

Most of the configuration used to connect to the Cumulocity MQTT service endpoint are the same as
the ones used to connect to the Cumulocity Core MQTT endpoint.

1. Configure Cumulocity URL

   <UserContext>
   
   ```sh
   sudo tedge config set c8y.url $C8Y_URL
   ```
   
   </UserContext>

   :::note
   Though you're setting the `c8y.url` config, the `c8y.mqtt_service.url` config is used under-the-hood for the connection,
   as this config is derived from the `c8y.mqtt` config, which is further derived from `c8y.url`, by default.
   For example, when `c8y.url` is `example.cumulocity.com`, `c8y.mqtt` would be `example.cumulocity.com:8883`
   and `c8y.mqtt_service.url` would be `example.cumulocity.com:9883`.

   If the MQTT service url is different from the one that would be derived from `c8y.url` or `c8y.mqtt`,
   set`c8y.mqtt_service.url` explicitly.
   :::

1. Enable connection to MQTT service endpoint

   ```sh
   sudo tedge config set c8y.mqtt_service.enabled true
   ```

1. Provide a topic to subscribe to

   ```sh
   sudo tedge config set c8y.mqtt_service.topics demo/topic
   ```

1. Make Cumulocity trust the device certificate as described [here](./connect.md#making-the-cloud-trust-the-device),
   if not already done.

1. Connect the device

   ```sh
   sudo tedge connect c8y
   ```

   This step establishes the bridge connection to both the core endpoint as well as the mqtt service endpoints simultaneously.

1. Once connected, all messages published to `c8y-mqtt/#` topics are forwarded to the MQTT service endpoint.

   ```sh
   tedge mqtt pub c8y-mqtt/test/topic '{ "hello": "world" }'
   ```

   The receipt of the published message can be validated on Cumulocity.

   :::note
   The bridge topic prefix `c8y-mqtt` can be changed using the tedge configuration: `c8y.mqtt_service.topic_prefix`.
   :::
