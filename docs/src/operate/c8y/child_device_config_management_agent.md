---
title: Child Device Configuration Management
tags: [Extend, Cumulocity, Child Device, Configuration]
sidebar_position: 6
---

# Enable configuration management on child devices

Configuration management can be enabled for child devices using the same `c8y-configuration-plugin`,
used for configuration management on thin-edge devices.
But, an additional piece of software must be developed by the child device owner,
to coordinate configuration management operations on it with the `c8y-configuration-plugin` running on thin-edge.
This software is referred to as "child device agent" in the rest of this document.

The child device agent needs to handle the following responsibilities:

* Declare the supported configuration list to thin-edge
* Handle configuration snapshot requests from thin-edge
* Handle configuration update requests from thin-edge

The *supported configuration list* is the list of configuration files on the child device
that needs to be managed from the cloud.
Configuration management by thin-edge is enabled *only* for the files provided in this list.
These declared configuration files can be fetched from thin-edge with config snapshot requests
and can be updated with config update requests.

Handling the above mentioned responsibilities involve multiple interactions with thin-edge
over MQTT to receive and respond to configuration management requests,
and HTTP to upload/download files while handling those requests.

For example, during the bootstrapping/startup of the child device,
the agent needs to upload the supported configuration list of the child device to thin-edge
by uploading a file using the HTTP `file-transfer` API of thin-edge,
followed by an MQTT message informing thin-edge that the upload completed.
Similarly, handling of a configuration snapshot or update request involves sending
MQTT messages before and after the configuration file is uploaded/downloaded via HTTP to/from thin-edge.

Since child device agents typically run on an external device and not on the thin-edge device itself,
the MQTT and HTTP APIs of thin-edge need to be accessed over the network using its IP address,
which is configured using the tedge configuration settings `mqtt.external.bind.address` or `mqtt.bind.address`.
The MQTT APIs are exposed via port 1883 and the HTTP APIs are exposed via port 8000.
In rare cases, where the child device agent is installed alongside thin-edge on the same device,
these APIs can be accessed via a local IP or even `127.0.0.1`.

The following sections explain the child device agent responsibilities in detail.

## Declare supported configuration list to thin-edge

The supported configuration list should be sent to thin-edge during the startup/bootstrap phase of the child device agent.
This bootstrapping is a 3 step process:

1. Prepare a `c8y-configuration-plugin.toml` file with the supported configuration list
1. Upload this file to thin-edge via HTTP
1. Notify thin-edge about the upload via MQTT

The child device agent needs to capture the list of configuration files that needs be managed from the cloud
in a `c8y-configuration-plugin.toml` file in the same format as specified in the [configuration management documentation](config_management_plugin.md) as follows:

```toml title="file: c8y-configuration-plugin.toml"
files = [
  {path = '/path/to/some/config', type = 'config1'},
  {path = '/path/to/another/config', type = 'config2'},
]
```

* `path` is the full path to the configuration file on the child device file system.
* `type` is a unique alias for each file entry which will be used to represent that file in Cumulocity UI

The child device agent needs to upload this file to thin-edge with an HTTP PUT request
to the URL: `http://{tedge-ip}:8000/tedge/file-transfer/{child-id}/c8y-configuration-plugin`

* `{tedge-ip}` is the IP of the thin-edge device which is configured as 
`mqtt.external.bind.address` or `mqtt.bind.address` or `127.0.0.1` if neither is configured.
* `{child-id}` is the child-device-id

Once the upload is complete, the agent should notify thin-edge about the upload by sending the following MQTT message:

```sh te2mqtt
tedge mqtt pub 'tedge/{child-d}/commands/res/config_snapshot' '{"type": "c8y-configuration-plugin", "path": "/child/local/fs/path"}'
```

## Handle config snapshot requests from thin-edge

Handling config snapshot requests from thin-edge is a 4-step process:

1. Subscribe to, and receive config snapshot requests via MQTT
1. Send an "executing" operation status update to acknowledge the receipt of the request via MQTT
1. Upload the requested config file to the URL received in the request via HTTP
1. Send a "successful" operation status update via MQTT

These steps are explained in detail below:

The child device agent must subscribe to the `tedge/{child-d}/commands/req/config_snapshot` MQTT topic
to receive the config snapshot requests from thin-edge.
These requests arrive in the following JSON format:

```json
{
  "type": "{config-type}",
  "path": "/child/local/fs/path",
  "url": "http://{tedge-ip}:8000/tedge/file-transfer/{child-d}/config_snapshot/{config-type}"
}
```

The `type` and `path` fields are the same values that the child device sent to thin-edge in its `c8y-configuration-plugin.toml` file.
The `url` value is what the child device agent must use to upload the requested config file.

On receipt of the request, the agent must send an "executing" MQTT status message as follows:

```sh te2mqtt
tedge mqtt pub tedge/{child-d}/commands/res/config_snapshot '{
  "status": "executing",
  "type": "{config-type}",
  "path": "/child/local/fs/path" 
}'
```

After sending this status message, the agent must upload the requested configuration file content to
the `url` received in the request with an HTTP PUT request.

Once the upload is complete, send a "successful" MQTT status message as follows:

**Topic**

```text
tedge/{child-d}/commands/res/config_snapshot
```

**Payload**

```json
{
  "status": "successful",
  "type": "{config-type}",
  "path": "/child/local/fs/path" 
}
```

If there are any failures while reading or uploading the requested config file,
a "failed" status update must be sent instead, to the same topic as follows:

```json
{
  "status": "failed",
  "type": "{config-type}",
  "path": "/child/local/fs/path" 
}
```

## Handle config update requests from thin-edge

Handling config update requests from thin-edge is a 5-step process:

1. Subscribe to, and receive config update requests via MQTT
1. Send an "executing" operation status update to acknowledge the receipt of the request via MQTT
1. Download the config file update from the URL received in the request via HTTP
1. Apply the config file update on the child device
1. Send a "successful" operation status update via MQTT

The child device agent must subscribe to the `tedge/{child-d}/commands/req/config_update` MQTT topic
to receive the config update requests from thin-edge.
These requests arrive in the following JSON format:

```json
{
  "type": "{config-type}",
  "path": "/child/local/fs/path",
  "url": "http://{tedge-ip}:8000/tedge/file-transfer/{child-d}/config_update/{config-type}"
}
```

The child device agent must download the config file update for the given `type` from thin-edge using the `url`.

On receipt of the request, the agent must send an "executing" MQTT status message as follows:

**Topic**

```text
tedge/{child-d}/commands/res/config_update
```

**Payload**

```json
{
  "status": "executing",
  "type": "{config-type}",
  "path": "/child/local/fs/path" 
}
```

After sending this status message, the agent must download the configuration file update
from the `url` received in the request with an HTTP GET request.
The agent can then apply the downloaded configuration file update on the device.

Once the update is applied, send a "successful" MQTT status message as follows:

**Topic**

```text
tedge/{child-d}/commands/res/config_update
```

**Payload**

```json
{
  "status": "successful",
  "type": "{config-type}",
  "path": "/child/local/fs/path" 
}
```

If there are any failures while downloading and applying the update,
a "failed" status update must be sent instead, to the same topic as follows:

```json
{
  "status": "failed",
  "type": "{config-type}",
  "path": "/child/local/fs/path" 
}
```

## References

* Configuration Management [documentation](config_management_plugin.md)
* Reference implementation of a [child device agent](https://github.com/thin-edge/thin-edge.io_examples/tree/main/child-device-agent) written in Python to demonstrate the contract described in this document.

