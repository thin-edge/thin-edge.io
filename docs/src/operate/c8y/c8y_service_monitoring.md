---
title: Health Monitoring
tags: [Operate, Cumulocity, Monitoring]
sidebar_position: 8
---

# How to monitor health of service from Cumulocity IoT

The health of a `thin-edge.io` service or any other `service` that is running on the `thin-edge.io` device
or on the `child` device can be monitored from the **Cumulocity IoT** by sending the `health-status` message to **Cumulocity IoT**.

## Send the health status of a service to health topic.

The table below lists the MQTT topics to which the health status message should be sent, and the
health status message format for both the `thin-edge` and for the `child` device.

|Device-type|Thin-edge-Health-status-mqtt-topic|Health-status-message|
|------|------------------------|---------------------|
|Thin-edge-device|`tedge/health/<service-name>`|`{"status":"status of the service","type":"type of service"}`|
|Child-device|`tedge/health/<child-device-id>/<service-name>`|`{"status":"status of the service","type":"type of service"}`|

:::note
The `status` here can be `up or down` or any other string. For example, `unknown`.
:::

For example, to monitor the health status of a `tedge-mapper-c8y` service that is running on a `thin-edge.io` device
one has to send the below message.

```sh te2mqtt
tedge mqtt pub tedge/health/tedge-mapper-c8y '{"status":"up","type":"thin-edge.io"}' -q 2 -r
```

The above message says that the `tedge-mapper-c8y` is `up` and the `type` of the service is `thin-edge.io`.


To monitor the health of a `docker` service that is running on an `external-sensor` child device,

```sh te2mqtt
tedge mqtt pub tedge/health/external-sensor/docker '{"status":"up","type":"systemd"}' -q 2 -r
```

:::note
The health status message has to be sent as a `retain` message.
:::

When an empty health status message is sent, e.g. `{}` or `''`, the `status` will be replaced with `unknown` and the `type` will be replaced with default value `service`.

## Conversion of the health status message to Cumulocity IoT service monitor message

The `tedge-mapper-c8y` will translate the `health status` message that is received on `tedge/health/#`
topic to `Cumulocity` specific `service monitor` message and sends it to `Cumulocity` cloud.

The table below gives more information about the **Cumulocity IoT** topic and the translated service monitor message for both `thin-edge` as well as for `child` device.

|Device-type|Cumulocity topic|Cumulocity smartrest message|
|------|------------------------|---------------------|
|Thin-edge-device|`c8y/s/us`|`102,<unique-service-id>,type,service-name,status`|
|Child-device|`c8y/s/us/<child-id>`|`102,<unique-service-id>,type,service-name,status`|

:::note
The `unique-service-id` for thin-edge device will be  `<device-name>_<service-name>`.

In case of child device `<device-name>_<child-id>_<service-name>`.

Note also that `102` is the `smartrest` template number for the service monitoring message.
:::

## Configuring the default service type

The `default service type` can be configured using the `tedge` cli.

The example below shows how one can set the default service type to `systemd`.

```sh
sudo tedge config set service.type systemd
```

:::note
When the `service type` was not sent with the `health status` message, then the configured default value will be used by
the mapper while translating the `health status` message to `service status` message.
:::

To clear the configured default service type one can use the command below.
This will set the `service.type` to `service`.

```sh
sudo tedge config unset service.type
```

# References

More info about the service monitoring can be found in the below link

[Service monitoring Cumulocity IoT](https://cumulocity.com/guides/reference/smartrest-two/#service-creation-102)
