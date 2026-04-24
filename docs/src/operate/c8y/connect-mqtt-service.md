---
title: Connecting to Cumulocity MQTT Service
tags: [Operate, Cloud, Connection, Cumulocity]
description: Connecting %%te%% to Cumulocity
sidebar_position: 1
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing relevant settings which will be reflected in the instructions. It makes it even easier to explore and use %%te%%.

<UserContextForm settings="DEVICE_ID,C8Y_URL" />

The user context will be persisted in your web browser's local storage.
:::

## MQTT Service

By default, %%te%% connects to Cumulocity via its [Core MQTT](https://cumulocity.com/docs/device-integration/mqtt/)
endpoint, which restricts devices to a predefined set of topics and data formats (e.g. SmartREST and JSON over MQTT).
The [Cumulocity MQTT service](https://cumulocity.com/docs/device-integration/mqtt-service) on the other hand,
is the next-gen MQTT endpoint offered by Cumulocity,
which allows devices to publish and receive data using user-defined custom topic and payload formats as well.
The Cumulocity MQTT Service is also capable of supporting both user-defined topics, as well as support for the SmartREST topics, however this
compatibility layer, is not available for production usage just yet.

This page explains how to connect %%te%% to the Cumulocity MQTT service so that devices can publish and subscribe to arbitrary topics on the cloud.

For more information about the possible ways to connect to the different MQTT interfaces offered by Cumulocity, see the following table.

|Name|Port|Description|Tenant feature required|Status|
|----|----|-----------|----------------------|------|
|[Core MQTT](https://cumulocity.com/docs/device-integration/mqtt/)|8883|Allows devices to send messages directly into Cumulocity, provided that the device implements the pre-defined topic schema and payload formats of Core MQTT|None|[General Availability](https://cumulocity.com/docs/2024/glossary/g/#ga)|
|[MQTT Service](https://cumulocity.com/docs/device-integration/mqtt-service/) (without SmartREST)|9883|Arbitrary topic and payload support via a second dedicated bridge connection alongside the Core MQTT bridge|None|[General Availability](https://cumulocity.com/docs/2024/glossary/g/#ga)|
|[MQTT Service](https://cumulocity.com/docs/device-integration/mqtt-service/) (with SmartREST)|9883|Arbitrary topic and payload support via a single bridge connection, replacing the Core MQTT bridge|mqtt-service.smartrest|[Public Preview](https://cumulocity.com/docs/2024/glossary/p/#public-preview)|

:::warning
Any option or feature not marked as [Generally Available](https://cumulocity.com/docs/2024/glossary/g/#ga) is subject to change and should not be used in production environments.
:::

## Option 1: MQTT Service (without SmartREST) {#without-smartrest}

In this approach, the existing %%te%% connection continues to use the Cumulocity Core MQTT endpoint,
and a second bridge is set up specifically to connect to the MQTT service endpoint.
This uses the community package [`tedge-mapper-c8y-mqttservice`](https://github.com/thin-edge/tedge-mapper-c8y-mqttservice).

1. Make sure the device is already connected to Cumulocity. If not, follow the [connection guide](./connect.md).

1. Configure the community repository on your device. Depending on how you installed %%te%% this might already be configured, however if it doesn't, you can configure the **community** repository by following the "Set Me Up" instructions on the [Cloudsmith](https://cloudsmith.io/~thinedge/repos/community/packages/) website.

1. Install the `tedge-mapper-c8y-mqttservice` community package

   You can install the package manually via the command line, or install it via Cumulocity's Software Management feature.

   ```sh tab={"label":"Debian/Ubuntu"}
   sudo apt-get install tedge-mapper-c8y-mqttservice
   ```

   ```sh tab={"label":"RHEL/Fedora/RockyLinux"}
   sudo dnf install tedge-mapper-c8y-mqttservice
   ```

   ```sh tab={"label":"Alpine"}
   sudo apk add tedge-mapper-c8y-mqttservice
   ```

   During installation, if the device is already configured to connect to Cumulocity, then it will automatically detect the configured url, and use the Cumulocity MQTT Service port, so in normal cases there shouldn't be any manual configuration required.

   Refer to the
   [community project's repository](https://github.com/thin-edge/tedge-mapper-c8y-mqttservice) for additional information.

1. If needed, adjust the `/etc/tedge/mappers/c8y-mqttservice/mapper.toml` configuration file to fine-tune which topics are bridged to and from
   the MQTT service endpoint.

1. If you're using SystemD, the service should start automatically, however you can manually start it using:

   ```sh
   sudo systemctl start tedge-mapper-c8y-mqttservice
   ```

## Option 2: MQTT Service (with SmartREST) {#with-smartrest}

In this approach, a single bridge connection is used for both the Cumulocity Core MQTT topics (e.g. SmartREST and JSON over MQTT)
and the MQTT service freeform topics. This reduces bandwidth overhead because only one MQTT connection is
maintained, eliminating the network overhead that a second bridge would introduce.

:::caution
This option requires the `mqtt-service.smartrest` tenant feature to be enabled on your Cumulocity tenant.
This feature is currently in [Public Preview](https://cumulocity.com/docs/2024/glossary/p/#public-preview)
and is subject to change. It should not be used in production until the feature reaches General Availability.
Contact your Cumulocity tenant administrator to have it enabled.
:::

### 1. Enable the tenant feature

Use [go-c8y-cli](https://github.com/reubenmiller/go-c8y-cli) to enable the required tenant feature, or you can use the Cumulocity Administration application:

```sh
c8y features enable --key mqtt-service.smartrest
```

### 2. Configure the device

1. Configure Cumulocity URL, if not already set.

   <UserContext>

   ```sh
   sudo tedge config set c8y.url $C8Y_URL

   # or if you're already using a custom c8y.http domain name
   sudo tedge config set c8y.mqtt $C8Y_URL
   ```

   </UserContext>

   :::note
   Though the `c8y.url` config is set in this step, the `c8y.mqtt` config is used under-the-hood for the connection,
   as this config is derived from `c8y.url`, by default.

   If the MQTT service url is different from the one that would be derived from `c8y.url`,
   then set `c8y.mqtt` explicitly.
   :::

1. Configure %%te%% to specify that the additional MQTT service related bridge rules should be enabled

   ```sh
   sudo tedge config set c8y.mqtt_service.enabled true
   ```

   :::note
   The `c8y.mqtt` value is derived differently based on whether mqtt service is enabled or not.
   For example, when `c8y.url` is `example.cumulocity.com` and when `mqtt_service` is enabled,
   `c8y.mqtt` would be derived as `example.cumulocity.com:9883` (the default mqtt service endpoint).
   else it would be `example.cumulocity.com:8883` (the default core mqtt endpoint).
   :::

1. Provide any topics that the device should subscribe to (e.g: topic to receive sensor config updates)

   ```sh
   sudo tedge config set c8y.mqtt_service.topics "sensors/temperature/set-config,foo/bar"
   ```

   Or if you only want to add an addition topic to the already configured values, then use `tedge config add`:

   ```sh
   sudo tedge config add c8y.mqtt_service.topics "sensors/temperature/set-config"
   sudo tedge config add c8y.mqtt_service.topics "foo/bar"
   ```

1. Make Cumulocity trust the device certificate as described [here](./connect.md#making-the-cloud-trust-the-device),
   if not already done.

1. Connect the device

   ```sh
   sudo tedge connect c8y
   ```

   This step establishes the bridge connection to the MQTT service endpoint instead of the Core MQTT endpoint.
   All MQTT traffic using both the built-in topics (e.g: SmartREST) as well as the user-provided custom topics
   are routed to the MQTT service endpoint, completely bypassing the Core MQTT endpoint.

   :::tip
   If the device was previously connected to Cumulocity (the Core MQTT endpoint), then you can just run `sudo tedge reconnect c8y` after steps 2 and 3.
   :::

## Publishing and subscribing to MQTT service topics

Once connected, all messages published to `c8y/mqtt/out/#` topics are forwarded to the MQTT service endpoint,
without the `c8y/mqtt/out/` prefix.

For example, to publish the temperature measurement:

```sh
tedge mqtt pub c8y/mqtt/out/sensors/temperature/measurement 25
```

The message will be published to the `sensors/temperature/measurement` topic on the MQTT service.

Similarly, any messages published to a subscribed topic on Cumulocity (e.g. `sensors/temperature/set-config`)
are published to the corresponding local bridge topic with a `c8y/mqtt/in/` prefix.

To see the set configuration commands received from the cloud, use the following command:

```sh
tedge mqtt sub c8y/mqtt/in/sensors/temperature/set-config
```

## Inspecting and testing the bridge mapping

### Inspect bridge mapping rules

To display the full set of topic mapping rules for the Cumulocity bridge, run:

<Tabs groupId="option">
  <TabItem value="Option 1 (without SmartREST)" label="Option 1 (without SmartREST)" default>

```sh
tedge bridge inspect c8y-mqttservice
```

  </TabItem>

  <TabItem value="Option 2 (with SmartREST)" label="Option 2 (with SmartREST)" default>

```sh
tedge bridge inspect c8y
```

  </TabItem>
</Tabs>


This is useful to understand which local topics map to which remote topics (and vice versa).

### Test a topic mapping

To check how a specific local topic will be mapped before publishing, use `tedge bridge test`:

<Tabs groupId="option">
  <TabItem value="Option 1 (without SmartREST)" label="Option 1 (without SmartREST)" default>

```sh
tedge bridge test c8y-mqttservice c8y/mqtt/out/foo/bar
```

```text title="Output"
Bridge configuration for Cumulocity
Reading from: /etc/tedge/mappers/c8y-mqttservice/bridge

[local] c8y/mqtt/out/foo/bar  ->  [remote] foo/bar (outbound)
  matched by rule: c8y/mqtt/out/# -> #
```

  </TabItem>

  <TabItem value="Option 2 (with SmartREST)" label="Option 2 (with SmartREST)" default>

```sh
tedge bridge test c8y c8y/mqtt/out/foo/bar
```

```text title="Output"
Bridge configuration for Cumulocity
Reading from: /etc/tedge/mappers/c8y/bridge

[local] c8y/mqtt/out/foo/bar  ->  [remote] foo/bar (outbound)
  matched by rule: c8y/mqtt/out/# -> #
```

  </TabItem>
</Tabs>

This confirms that a message published locally to `c8y/mqtt/out/foo/bar` will be forwarded to the `foo/bar` topic on the MQTT service.
