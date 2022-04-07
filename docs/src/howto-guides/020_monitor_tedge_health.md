# How to monitor health of tedge daemons

The health of tedge daemon processes like `tedge-mapper`, `tedge-agent` etc can be monitored via MQTT.
These daemons expose MQTT health endpoints which you can query to check if the process is still active or not.

The health endpoints conform to the following topic scheme, listening for health check requests:

`tedge/health-check/<tedge-daemon-name>`

expecting empty messages, triggering the health check.

The daemon will then respond back on the topic:

`tedge/health/<tedge-daemon-name>`

with the following payload:

```json
{ "status": "up", "pid": <process id of the daemon> }
```

All daemons will also respond to health checks sent to the common health check endpoint `tedge/health-check`.

## Supported MQTT topic endpoints

The following endpoints are currently supported by various tedge daemons:

* `tedge/health/tedge-agent`
* `tedge/health/tedge-mapper-c8y`
* `tedge/health/tedge-mapper-az`
* `tedge/health/tedge-mapper-collectd`

All future tedge daemons will also follow the same topic naming scheme convention.

# Mosquitto bridge health endpoints

The mosquitto bridge clients connecting thin-edge devices to the respective cloud platforms also report their health status as retained messages to `tedge/health/<mosquitto-cloud-bridge>` topics.
The health check messages published by these clients are just numeric values `1` or `0`, indicating active and dead bridge clients respectively.

Here are the health endpoints of curently supported clouds, bridged with mosquitto:

| Cloud      | Health topic                        |
| ---------- | ----------------------------------- |
| Cumulocity | `tedge/health/mosquitto-c8y-bridge` |
| Azure      | `tedge/health/mosquitto-az-bridge`  |

Explicit health check requests via `tedge/health-check` topics is not supported by these bridge clients.
Since the health status messages are sent as retained messages, just subscribing to these health topics is sufficient to get the latest status.
