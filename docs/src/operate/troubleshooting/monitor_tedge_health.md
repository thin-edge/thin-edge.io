---
title: Monitoring Service Health
tags: [Operate, Monitoring]
sidebar_position: 1
---

# How to monitor health of tedge services

The health of tedge services like `tedge-mapper`, `tedge-agent` etc can be monitored via MQTT.
These services expose MQTT health endpoints which you can query to check if the process is still active or not.

To get the last known health status of a service you can subscribe to the following topic

```text
tedge/health/<tedge-service-name>
```

To refresh the health status of the service, publish an empty message on the topic below.

```text
tedge/health-check/<tedge-service-name>
```

:::note
If the response is not received then most likely the service is down, or not responding
:::


For example, `tedge-mapper-c8y` publishes below message on topic `tedge/health/tedge-mapper-c8y` when it starts

```json
{"pid":290854,"status":"up","time":1674739912}
```

|Property|Description|
|--------|-----------|
|`pid`|Process ID of the service|
|`status`|Service status. Possible values are `up` or `down`|
|`time`|Unix timestamp in seconds|

If the tedge service gets stopped or crashed or get killed then a `down` message will be published on health status topic
and this will be retained till the tedge service is re-launched.

E.g the mapper being killed:

```sh te2mqtt
tedge mqtt sub 'tedge/health/#'
```

```log title="Output"
INFO: Connected
[tedge/health/mosquitto-c8y-bridge] 1
[tedge/health/tedge-mapper-c8y] {"pid":51367,"status":"down"}
[tedge/health/tedge-agent] {"pid":13280,"status":"up","time":1675330667}
```
## Supported MQTT health endpoint topics

The following endpoints are currently supported:

* `tedge/health/tedge-agent`
* `tedge/health/tedge-mapper-c8y`
* `tedge/health/tedge-mapper-az`
* `tedge/health/tedge-mapper-aws`
* `tedge/health/tedge-mapper-collectd`
* `tedge/health/c8y-log-plugin`
* `tedge/health/c8y-configuration-plugin`

All future tedge services will also follow the same topic naming scheme convention.

# Mosquitto bridge health endpoints

The mosquitto bridge clients connecting thin-edge devices to the respective cloud platforms also report their health status as retained messages to `tedge/health/<mosquitto-cloud-bridge>` topics.
The health check messages published by these clients are just numeric values `1` or `0`, indicating active and dead bridge clients respectively.

Here are the health endpoints of currently supported clouds, bridged with mosquitto:

| Cloud      | Health topic                        |
| ---------- | ----------------------------------- |
| Cumulocity | `tedge/health/mosquitto-c8y-bridge` |
| Azure      | `tedge/health/mosquitto-az-bridge`  |
| AWS        | `tedge/health/mosquitto-aws-bridge` |

Explicit health check requests via `tedge/health-check` topics is not supported by these bridge clients.
Since the health status messages are sent as retained messages, just subscribing to these health topics is sufficient to get the latest status.
