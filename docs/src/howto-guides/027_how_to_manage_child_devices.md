# Configuration management for child devices

After following this how to you will have an overview of the configurations management for child devices. As an example, a Raspberry Pi is used. This how to explains in small steps to reach the goal of getting configuration snapshot from child device as well as sending configuration to the child device.

## Introduction
thin-edge.io is an open-source project to provide a cloud-agnostic edge framework. It is much more generic than the device management agent, so it can connect to multiple IoT cloud platforms, and it allows flexible logic executed on the device. It is optimized for a very small footprint and high performance.

The Raspberry PI is a relatively simple and cheap device but powerful. Therefore it is ideal for testing and try-outs and some production use cases.

 Configuration management can be enabled for child devices using the same ***c8y-configuration-plugin***,
used for configuration management on thin-edge devices.

Child device has to be able to handle following responsibilities:

* Declare the supported configuration list to thin-edge
* Handle configuration snapshot requests from thin-edge
* Handle configuration update requests from thin-edge

**Please note:** Any peace of software that will handle the above responsibilities is not part of the thin-edge.io. This software is referred to as ***"child device agent"*** in the rest of this document.

The supported configuration list is the list of configuration files on the child device that needs to be managed from the cloud. Configuration management by thin-edge is enabled only for the files provided in this list. These declared configuration files can be fetched from thin-edge with config snapshot requests and can be updated with config update requests.

Handling the above mentioned responsibilities involve multiple interactions with thin-edge over MQTT to receive and respond to configuration management requests, and HTTP to upload/download files while handling those requests.

For example, during the bootstrapping/startup of the child device, the agent needs to upload the supported configuration list of the child device to thin-edge by uploading a file using the HTTP file-transfer API of thin-edge, followed by an MQTT message informing thin-edge that the upload completed. Similarly, handling of a configuration snapshot or update request involves sending MQTT messages before and after the configuration file is uploaded/downloaded via HTTP to/from thin-edge.

Since child device agents typically run on an external device and not on the thin-edge device itself, the MQTT and HTTP APIs of thin-edge need to be accessed over the network using its IP address, which is configured using the tedge configuration settings mqtt.external.bind_address or mqtt.bind_address. The MQTT APIs are exposed via port 1883 and the HTTP APIs are exposed via port 8000. In rare cases, where the child device agent is installed alongside thin-edge on the same device, these APIs can be accessed via a local IP or even 127.0.0.1.

##  Prerequisite

To follow this how to guide, you only need the following:

- A [Cumulocity IoT](https://www.softwareag.cloud/site/product/cumulocity-iot.html) tenant.
- A Raspberry Pi (3 or 4) with Raspian installed, configured and connected to the Cumulocity tenant. For other boards and OS'es have a look [here](https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/supported-platforms.md) 
- Updated device:

```
$ sudo apt-get update && sudo apt-get upgrade
```
## Steps

This how to guid is divided into small steps. 

[Step 1 Startup / Bootstrap of the child device](#Startup-/-Bootstrap-of-the-child-device)

[Step 2 Handle config snapshot request from thin-edge](#handle-config-snapshot-request-from-thin-edge)

[Step 3 Handle config update request from thin-edge](#handle-config-update-request-from-thin-edge)


### Step 1 Startup / Bootstrap of the child device

The supported configuration list should be sent to thin-edge during the startup/bootstrap phase of the child device agent.
This bootstrapping is a 3 step process:

1. Prepare a c8y-configuration-plugin.toml file with the supported configuration list
1. Upload this file to thin-edge via HTTP
1. Notify thin-edge about the upload via MQTT

The child device agent needs to capture the list of configuration files that needs be managed from the cloud in a c8y-configuration-plugin.toml file in the same format as specified in the [configuration management documentation](https://thin-edge.github.io/thin-edge.io/html/howto-guides/025_config_management_plugin.html) as follows:

```toml
files = [
    { path = '/path/to/some/config', type = 'config1'},
]
```
* `path` is the full path to the configuration file on the child device file system.
* `type` is a unique alias for each file entry which will be used to represent that file in Cumulocity UI

Example:

```json
printf "files = [
       { path = '/home/pi/config', type = 'config1'},
]" > c8y-configuration-plugin 
```
The child device agent needs to upload this file to thin-edge with an HTTP PUT request to the URL:
`http://{tedge-ip}:8000/tedge/file-transfer/{child-id}/c8y-configuration-plugin`

* {tedge-ip} is the IP of the thin-edge device which is configured as mqtt.external.bind_address or mqtt.bind_address or 127.0.0.1 if neither is configured.
* {child-id} is the child-device-id

Example:

```toml
curl -X PUT http://192.168.1.120:8000/tedge/file-transfer/sensor1/c8y-configuration-plugin \
--data-binary @- << EOF
files = [
     { path = '/home/pi/config1', type = 'config1' },
]
EOF 
```
Once the upload is complete, the agent should notify thin-edge about the upload by sending the following MQTT message:

Topic:

tedge/{child-d}/commands/res/config_snapshot

Payload:

{ "type": "c8y-configuration-plugin”, "path": ”/child/local/fs/path”}

Example:

```toml
mosquitto_pub -h 192.168.1.100 -t "tedge/sensor01/commands/res/config_snapshot" -m '{"status": null,  "path": "", "type":"c8y-configuration-plugin", "reason": null}'
```
### Step 2 Handle config snapshot request from thin-edge
Handling config snapshot requests from thin-edge is a 4-step process:

**1. Subscribe to, and receive config snapshot requests via MQTT**

On Cumulocity IoT click following -> *Device Management* -> *All Devices* -> The in prerequisites configured and connected device -> *Child devices* -> the in Step1 defined child device (sensor1) -> *Configuration*

In the *Configurations* tab on the *DEVICE-SUPPORTED CONFIGURATIONS* click on the in Step1 created config file (`config1`) and clicking on ***Get snapshot from device*** will triger receiving config snapshot requests via MQTT

![Snapshot](/Users/glis/Pictures/Screenshots/Snapshot.png)

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

**2. Send an “executing” operation status update to acknowledge the receipt of the request via MQTT**

On receipt of the request, the agent must send an "executing" MQTT status message as follows:

**Topic:**

`tedge/{child-d}/commands/res/config_snapshot`

**Payload**:

```json
{
    "status": "executing",
    "type": "{config-type}",
    "path": "/child/local/fs/path" 
}
```

Example:

```toml
mosquitto_pub -h 192.168.1.120 -t "tedge/sensor1/commands/res/config_snapshot" -m '{"status": "executing", "path": "/home/pi/config1", "type": "config1", "reason": null}'
```


**3. Upload the requested config file to the URL received in the request via HTTP**

After sending this status message, the agent must upload the requested configuration file content to the url received in the request with an HTTP PUT request.

Example:

```toml
curl -X PUT --data-binary @/home/pi/config1 http://192.168.1.120:8000/tedge/file-transfer/sensor1/config_snapshot/config1'
```

**4.Send a “successful” operation status update via MQTT**

Once the upload is complete, send a "successful" MQTT status message as follows:

**Topic:**

`tedge/{child-d}/commands/res/config_snapshot`

**Payload**:

```json
{
    "status": "successful",
    "type": "{config-type}",
    "path": "/child/local/fs/path" 
}
```

Example:

```toml
mosquitto_pub -h 192.168.1.120 -t "tedge/sensor1/commands/res/config_snapshot" -m '{"status": "successful", "path": "/home/pi/config1", "type": "config1", "reason": null}'
```

### Step 3 Handle config update request from thin-edge

Handling config update requests from thin-edge is a 5-step process:

**1. Subscribe to, and receive config update requests via MQTT**

On Cumulocity IoT click following -> *Device Management* -> *All Devices* -> The in prerequisites configured and connected device -> *Management* -> *Configuration repository* -> *Add configuration snapshot* and add/upload a configuration snapshot (config2)

Click on *All Devices* -> The in prerequisites configured and connected device -> *Child devices* -> the in Step1 defined child device (sensor1) -> *Configuration*

In the *Configurations* tab on the *DEVICE-SUPPORTED CONFIGURATIONS* click on the in Step1 created config file (`config1`)

In the *Configurations* tab on the *AVAILABLE SUPPORTED CONFIGURATIONS* click on the previously added/uploaded configuration snapshot (config2) and clicking on ***Send configuration to device*** will triger receiving config snapshot requests via MQTT

![Snapshot](/Users/glis/Pictures/Screenshots/Config.png)

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
**2. Send an “executing” operation status update to acknowledge the receipt of the request via MQTT**

On receipt of the request, the agent must send an "executing" MQTT status message as follows:

**Topic:**

`tedge/{child-d}/commands/res/config_update`

**Payload**:

```json
{
    "status": "executing",
    "type": "{config-type}",
    "path": "/child/local/fs/path" 
}
```

Example:

```toml
mosquitto_pub -h 192.168.1.120 -t "tedge/sensor1/commands/res/config_update" -m '{"status": "executing", "path": "/home/pi/config2", "type": "config2", "reason": null}'
```
**3.Download the config file update from the URL received in the request via HTTP**

After sending this status message, the agent must download the configuration file update
from the `url` received in the request with an HTTP GET request.
The agent can then apply the downloaded configuration file update on the device.

Example:

```toml
curl http://192.168.1.120:8000/tedge/file-transfer/sensor1/config_update/config2 --output config2
```
**4. Apply the config file update on the child device**

The agent can then apply the downloaded configuration file update on the device.

**5. Send a “successful” operation status update via MQTT**

Once the update is applied, send a "successful" MQTT status message as follows:

**Topic:**

`tedge/{child-d}/commands/res/config_update`

**Payload**:

```json
{
    "status": "successful",
    "type": "{config-type}",
    "path": "/child/local/fs/path" 
}
```
Example:

```toml
mosquitto_pub -h 192.168.1.120 -t "tedge/sensor1/commands/res/config_update" -m '{"status": "successful", "path": "/home/pi/config2", "type": "config2", "reason": null}'
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

Example:

```toml
mosquitto_pub -h 192.168.1.120 -t "tedge/sensor1/commands/res/config_update" -m '{"status": "failed", "path": "/home/pi/config2", "type": "config2", "reason": null}'
```

## References

* Configuration Management [documentation](./025_config_management_plugin.md)
* Reference implementation of a [child device agent](https://github.com/thin-edge/thin-edge.io_examples/tree/main/child-device-agent) written in Python to demonstrate the contract described in this document.

