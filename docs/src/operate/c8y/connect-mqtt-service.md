---
title: 🚧 Connecting to Cumulocity MQTT Service
tags: [Operate, Cloud, Connection, Cumulocity]
description: Connecting %%te%% to Cumulocity
sidebar_position: 1
draft: true
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';
import BrowserWindow from '@site/src/components/BrowserWindow';

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing relevant settings which will be reflected in the instructions. It makes it even easier to explore and use %%te%%.

<UserContextForm settings="DEVICE_ID,C8Y_URL,C8Y_TENANT_ID,C8Y_USER,C8Y_PASSWORD" />

The user context will be persisted in your web browser's local storage.
:::

## MQTT Service

Cumulocity supports a generic [MQTT service endpoint](https://cumulocity.com/docs/device-integration/mqtt-service)
that allows users to send free-form messages (any arbitrary topic and payload) to it.
It does not replace the existing core MQTT endpoint and hence needs to be connected to, separately.
The following sections cover steps required to establish a connection to the Cumulocity MQTT service endpoint.

:::caution
Cumulocity MQTT service is still a work-in-progress and the external interfaces are bound to change in future.
The %%te%% interfaces might also change accordingly.
:::

### Configure the device

Most of the configurations used to connect to the MQTT service endpoint of Cumulocity are same as
the ones used to connect to the core MQTT endpoint.

1. Configure Cumulocity tenant URL:

   <UserContext>
   
   ```sh
   sudo tedge config set c8y.url $C8Y_URL
   ```
   
   </UserContext>

1. If you're using device certificates to connect to Cumulocity, set the tenant id (skip this when using username/password):

   <UserContext>
   
   ```sh
   sudo tedge config set c8y.tenant_id $C8Y_TENANT_ID
   ```
   
   </UserContext>

1. Enable connection to MQTT service endpoint:

   ```sh
   sudo tedge config set c8y.mqtt_service.enabled true
   ```

1. Provide a topic to subscribe to (only a single topic is allowed at the moment):

   ```sh
   sudo tedge config set c8y.mqtt_service.topics demo/topic
   ```

### Configure the cloud to trust the device certificate

Follow the steps [here](./connect.md#making-the-cloud-trust-the-device) to make Cumulocity trust the device certificate.

### Connect the device

The device is connected to MQTT service endpoint along with the core MQTT connection to Cumulocity:

```sh
sudo tedge connect c8y
```

This step establishes the bridge connection to both the core endpoint as well as the mqtt service endpoints simultaneously.

### Validate the connection

Once connected, all messages published to `c8y-mqtt/#` topics are forwarded to the MQTT service endpoint.

To validate the same, connect a different MQTT client directly to the MQTT service endpoint (`mosquitto_sub` in this example)
and subscribe to the desired topic:

<UserContext>

```sh
mosquitto_sub -d -v -h $C8Y_URL -p 2883 -i test-client-123 -u $C8Y_TENANT_ID/$C8Y_USER -P $C8Y_PASSWORD -t sub/topic
```

</UserContext>

:::note
The test client must use a client id that is different from the device id.
:::

Once subscribed, publish to the same topic from the %%te%% device:

```sh
tedge mqtt pub c8y-mqtt/test/topic "hello world"
```

Now, validate that the same message is received by the test client.