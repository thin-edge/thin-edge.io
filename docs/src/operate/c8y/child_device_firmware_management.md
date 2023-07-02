---
title: Child Device Firmware Management
tags: [Operate, Cumulocity, Child Device, Firmware]
sidebar_position: 7
---

# Enable firmware management on child devices with Cumulocity

Firmware management can be enabled for child devices using the `c8y-firmware-plugin`.
This thin-edge plugin coordinates the firmware update operation handling with Cumulocity,
by establishing secure communication with the cloud,
managing firmware file downloads, which are typically large files, even over flaky networks,
caching the downloaded files for re-use across multiple child devices etc.
For more details on the inner workings of this plugin, refer to the [reference guide](../../references/c8y-firmware-management.md).

In order to install the firmware itself on the child device,
an additional piece of software must be developed by the child device owner as well,
to coordinate firmware installation on the device with the `c8y-firmware-plugin` running on thin-edge.
This software is referred to as `child-device-connector` in the rest of this document.

The responsibilities of the child device connector are:

* Receive firmware update requests from thin-edge
* Download and apply the updated firmware from thin-edge
* Send status updates on the progress of the firmware update operation to thin-edge

Handling the above mentioned responsibilities involves
multiple interactions with thin-edge over its MQTT and HTTP APIs.
In cases where the child device connector is installed alongside thin-edge on the same device,
these APIs can be accessed via a local IP or even `127.0.0.1`.
The MQTT APIs are exposed via port 1883 and the HTTP APIs are exposed via port 8000, by default.
When the child device connector is running directly on the external child device,
the MQTT and HTTP APIs of thin-edge need to be accessed over the network using its IP address and ports,
which are configured using the tedge config settings `mqtt.client.host` or `mqtt.client.port` for MQTT
and `http.address` and `http.port` for HTTP.

The `c8y-firmware-plugin` running on thin-edge, along with the `child-device-connector`
running either on the thin-edge device itself or directly on the child device,
provides firmware management support for that device.

## Declare firmware management support of child device

At first, the child device needs to declare that it supports firmware management from Cumulocity
using the [supported operations API](supported_operations.md) of thin-edge
by simply creating an empty operations file on the thin-edge device as follows:

```sh
sudo touch /etc/tedge/operations/c8y/<child-device-id>/c8y_Firmware
```

This action will add `c8y_Firmware` as a new supported operation for the child device in Cumulocity.

:::note
This initial bootstrapping step needs to be performed on the thin-edge device directly
and can not be done from the child device over the network.
An over-the-network API for the same could be provided in future releases. 
:::

# Handle firmware update requests from thin-edge

Once the firmware management operation is enabled for a child device,
it is ready to serve firmware update requests received from Cumulocity via thin-edge.

Handling firmware update requests from thin-edge is a 5-step process:

1. Subscribe to, and receive firmware update requests via MQTT
1. Send an "executing" operation status update to acknowledge the receipt of the request via MQTT
1. Download the firmware file update from the URL received in the request via HTTP
1. Apply the firmware file update on the child device
1. Send a "successful" operation status update via MQTT

The following sections cover these steps in detail.

### Subscribe to firmware update requests via MQTT

The child device connector must subscribe to the `tedge/{CHILD_ID}/commands/req/firmware_update` MQTT topic
to receive the firmware update requests from thin-edge.

**Example**

```sh
mosquitto_sub -h {TEDGE_DEVICE_IP} -t "tedge/{CHILD_ID}/commands/req/firmware_update"
```

These requests arrive in the following JSON format:

**Payload**

```json
{
  "id": "{OP_ID}",
  "attempt": 1,
  "name": "{FIRMWARE_NAME}",
  "version": "{FIRMWARE_VERSION}",
  "url":"http://{TEDGE_HTTP_ADDRESS}:{TEDGE_HTTP_PORT}/tedge/file-transfer/tedge-child/firmware_update/{FILE_ID}",
  "sha256":"{FIRMWARE_FILE_SHA256}"
}
```

**Example**

```json
{
  "id": "tLrLthPXksKbqRqIsrDmy",
  "attempt": 1,
  "name": "OpenWRT",
  "version": "22.03",
  "url":"http://127.0.0.1:8000/tedge/file-transfer/tedge-child/firmware_update/93d50a297a8c235",
  "sha256":"c036cbb7553a909f8b8877d4461924307f27ecb66cff928eeeafd569c3887e29"
}
```

The fields in the request are describe below.

|Property|Description|
|--------|-----------|
|id| A unique id for the request. All responses must be sent back with the same id.|
|attempt| This `attempt` count can be used to differentiate between fresh requests and re-sent requests, as this `attempt` count will be higher than `1` if the same request is resent from thin-edge on rare occasions (e.g: thin-edge gets restarted while the child device is processing a firmware request).|
|name| Name of the firmware package|
|version| Version of the firmware package|
|sha256| The SHA-256 checksum of the firmware binary served in the `url` which can be used to verify the integrity of the downloaded package post-download.|

### Send executing response via MQTT

On receipt of the request, the connector may optionally send an "executing" MQTT status message,
to acknowledge the receipt of the request, as follows:

**Topic**

```text
tedge/{CHILD_ID}/commands/res/firmware_update
```

**Payload**

```json
{
  "id": "{OP_ID}",
  "status": "executing"
}
```

where the `OP_ID` must match the `id` received in the `firmware_update` request.

**Example**

```sh
mosquitto_pub -h {TEDGE_DEVICE_IP} -t "tedge/{CHILD_ID}/commands/res/firmware_update" -m '{"id": "{OP_ID}", "status": "executing"}'
```

### Download the firmware package via HTTP

After sending this status message, the connector must download the firmware file
from the `url` received in the request with an HTTP GET request.
Validating the integrity of the downloaded package by matching its SHA-256 hash value
against the `sha256` checksum received in the request is highly recommended.

### Apply the firmware package

The connector can then apply the downloaded firmware file update on the device.
This step is very device specific and might require use of other device specific protocols as well.

### Send successful/failed status via MQTT

Once the update is successfully applied, send a "successful" MQTT status message as follows:

**Topic**

```text
tedge/{CHILD_ID}/commands/res/firmware_update
```

**Payload**

```json
{
  "id": "{OP_ID}",
  "status": "successful"
}
```

**Example**

```sh
mosquitto_pub -h {TEDGE_DEVICE_IP} -t "tedge/{CHILD_ID}/commands/res/firmware_update" -m '{ "id": "{OP_ID}", "status": "successful" }'
```

If there are any failures while downloading or applying the update,
a "failed" status update (with an optional `reason`) must be sent instead, to the same topic as follows:

**Payload**

```json
{
  "id": "{OP_ID}",
  "status": "failed",
  "reason": "Failure reason"
}
```

**Example**

```sh
mosquitto_pub -h {TEDGE_DEVICE_IP} -t "tedge/{CHILD_ID}/commands/res/firmware_update" -m '{ "id": "{OP_ID}", "status": "failed", "reason": "SHA-256 checksum validation failed" }'
```

## Cleanup

To save bandwidth, thin-edge downloads a single firmware file only once and keeps it cached for reuse across multiple child devices,
as firmware updates could be applied to a fleet of devices together.
The cached files are stored under the tedge data directory `/var/tedge/cache`, by default.
Since thin-edge does not know how many devices it will be reused for and for how long, it can not clean them up on its own.
So, the user must manually delete the cached firmware files once the update is complete on all child devices.

## References

* Reference implementation of a [child device connector](https://github.com/thin-edge/thin-edge.io_examples/tree/main/child-device-agent) written in Python to demonstrate the contract described in this document.
This connector supports both configuration and firmware management operations.
So, just focus on the firmware management aspects.
