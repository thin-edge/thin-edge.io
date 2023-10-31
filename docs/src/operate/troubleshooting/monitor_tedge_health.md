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
te/<service-topic-id>/status/health
```

To refresh the health status of the service, publish an empty message on the topic below.

```text
te/<service-topic-id>/cmd/health/check
```

:::note
If the response is not received then most likely the service is down, or not responding
:::


For example, `tedge-mapper-c8y` publishes a message on topic `te/device/main/service/tedge-mapper-c8y/status/health` when it starts:

```json
{ "pid": 290854, "status": "up", "time": "2023-04-02T21:37:12.345678901Z" }
```

<!-- TODO: this should be in a reference about health status messages -->

| Property | Description                                        |
|----------|----------------------------------------------------|
| `pid`    | Process ID of the service                          |
| `status` | Service status. Possible values are `up` or `down` |
| `time`   | Timestamp in RFC3339 format                        |

If the tedge service gets stopped, crashed, or killed, then a `down` message will be published on health status topic
and this will be retained until the service is restarted.

E.g. the mapper being killed:

```sh te2mqtt
tedge mqtt sub 'te/+/+/+/+/status/health'
```

```log title="Output"
INFO: Connected
[te/device/main/service/mosquitto-c8y-bridge/status/health] 1
[te/device/main/service/tedge-mapper-c8y/status/health] {"pid":51367,"status":"down"}
[te/device/main/service/tedge-agent/status/health] {"pid":13280,"status":"up","time":"2023-02-02T09:37:47+00:00"}
```
## Supported MQTT health endpoint topics

The following endpoints are currently supported:

* `te/device/main/service/tedge-agent/status/health`
* `te/device/main/service/tedge-mapper-c8y/status/health`
* `te/device/main/service/tedge-mapper-az/status/health`
* `te/device/main/service/tedge-mapper-aws/status/health`
* `te/device/main/service/tedge-mapper-collectd/status/health`
* `te/device/main/service/tedge-log-plugin/status/health`
* `te/device/main/service/tedge-configuration-plugin/status/health`

All future tedge services will also follow the same topic naming scheme convention.

## Mosquitto bridge health endpoints

The mosquitto bridge clients connecting thin-edge devices to the respective cloud platforms also report their health
status as retained messages to `te/device/main/service/<mosquitto-cloud-bridge>/status/health` topics. The health check
messages published by these clients are just numeric values `1` or `0`, indicating active and dead bridge clients
respectively.

Here are the health endpoints of currently supported clouds, bridged with mosquitto:

| Cloud      | Health topic                                                |
|------------|-------------------------------------------------------------|
| Cumulocity | `te/device/main/service/mosquitto-c8y-bridge/status/health` |
| Azure      | `te/device/main/service/mosquitto-az-bridge/status/health`  |
| AWS        | `te/device/main/service/mosquitto-aws-bridge/status/health` |

Explicit health check requests via `te/<bridge-service-topic-id>/cmd/health/check` topics is not supported by these bridge clients.
Since the health status messages are sent as retained messages, just subscribing to these health topics is sufficient to get the latest status.
