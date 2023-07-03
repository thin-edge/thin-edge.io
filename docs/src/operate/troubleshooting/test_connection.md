---
title: Testing the cloud connection
tags: [Operate, Cloud, Connection]
sidebar_position: 1
---

# How to test the connection to cloud?

We provide a way to test the connection from your device to a cloud provider.
You can call this connection check function by

```sh
sudo tedge connect <cloud> --test
```

It returns exit code 0 if the connection check is successful, otherwise, 1.

This test is already performed as part of the `tedge connect <cloud>` command.

## What does the test do?

The connection test sends a message to the cloud and waits for a response.
The subsequent sections explain the cloud-specific behaviour.

### For Cumulocity IoT

The test publishes [a SmartREST2.0 static template message for device creation `100`](https://cumulocity.com/guides/device-sdk/mqtt/#a-nameinventory-templatesinventory-templates-1xxa) to the topic `c8y/s/us`.
If the device-twin is already created in your Cumulocity,
the device is supposed to receive `41,100,Device already existing` on the error topic `c8y/s/e`.

So, the test subscribes to `c8y/s/e` topic and if it receives the expected message on the topic, the test is marked successful.

The connection test sends maximum two of SmartREST2.0 `100` requests.
This is because the first `100` request can be considered a successful device creation request if the device-twin does not exist in Cumulocity yet.

### For Azure IoT Hub

The test subscribes to the topic `az/twin/res/`.
Then, it publishes an empty string to the topic `az/twin/GET/?$rid=1`. 

If the connection check receives a message containing `200` (status success), the test is marked successful.

The connection test sends the empty string only once.


