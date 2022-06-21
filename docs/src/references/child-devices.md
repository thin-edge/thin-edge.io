# Child Solution Proposals

* Child-Device support is provided on thin-edge public interfaces as below:
    - tedge/measurements
    - tedge/alarms
    - tedge/events
    - SW Management plugin API
    - Configuration Management
    - Logging Management
    
* Each public interface that provides child-device support has an optional field "child-device ID".

* Interfaces available on MQTT (as measurements, alarms, events) manage the field "child-device ID" as last topic of a message.
  Examples:
  
  ```
     tedge/measurements/<child-device ID>
     tedge/events/<event-type>/<child-device ID>
  ```

- Interface available via CLI (as SW Management Plugin API) manage the field "child-device ID" as optional argument in the command call.
  That means the command is executed locally on the thin-edge device, and has to take care to execute the remote action on the child-device.

  Examples:
  ```
    /etc/tedge/sm-plugins/<plugin-name>     install foo v1.2 /path/to/file <child-device ID>
    /etc/tedge/sm-plugins/CODESYS-handler   install foo v1.2 /path/to/file <child-device ID>
    /etc/tedge/sm-plugins/serial-programmer install foo v1.2 /path/to/file <child-device ID>
  ```
  Alternative with child-device ID in the path (avoids conflicts between filename over (child)devices), and allows thin-edge to gather a list of child devices (e.g. to ask each SW Man plugins for a Software List):
  ```
    /etc/tedge/sm-plugins/<child-device ID>/CODESYS-handler   install foo v1.2 /path/to/file
  ```

- If empty or no child-device ID is given, the thin-edge device is assumed to be meant.


TODO: Add restriction: "Those interfaces that are not provided via network (e.g. just via CLI) are not accessible from an external child device. That means for those a local process on the thin-edge device would be required."

TODO: Multilevel childs are not yet covered.

## Device provisioning

### Requirements

1. Provision a device as a child device of a thin-edge device in cloud
2. During the initial device provisioning, the device can define its capabilities like:
   1. The configuration files that it supports
   2. The list of software that's installed on that device
   3. Any other metadata that describes other properties of the device like its type, firmware version, supported data types etc
3. Even after the initial provisioning, any of the above mentioned metadata like config list, software list etc can be updated
4. Ability to append additional metadata like a new config or a new software dynamically without having to provide the full set of configs or software every time an update is required

A child device's metadata is expressed as a composition of multiple JSON fields and fragments that describes the device's capabilities.
The device's capabilities/metadata is not just set once, but can be added/updated dynamically.

### Design

* A device only knows its immediate parent and NOT its grand parents in the hierarchy
* Parent device id declared during initialization
* A device is aware of all its children and grandchildren, so that it can subscribe for requests on their behalf.
  But it doesn't care about the actual hierarchical structure of the downstream grandchildren.
* All the metadata for a device need not set at once. Different metadata fragments can be appended in steps.
* If an incoming field already exists, then it is replaced by the new value
* If the connected cloud doesn't support metadata updates,
  then the corresponding cloud mapper needs to mimic "append" by fetching the current state and appending the new fragments to it
* The very first metadata message defines the parent device.
* If the parent is not defined, it is assumed to be the thin-edge device itself

E.g: Publish the metadata JSON to

`tedge/meta/<child-device-id>`

with payload 

```json
{
    "parent": "",
    "type": "codesys-plc",
    ...
}
```

If the configuration list of the device needs to be updated, it can be sent as another fragment message as follows:

```json
{
    "config-types": ["data-mapping-config", "another-config"]
}
```

**TBD**

* Dynamic removal of fragments

## Configuration management

### Requirements

1. Configuration management for different apps could be supported by different components/plugins
2. These components may not have a shared file system between them(some running as native processes, others as containers etc)

### Proposal 1

**Core design principles**

* All config binary exchanges between the cloud mappers and other processes happen via `tedge-agent`
* `tedge-agent` to expose REST APIs for any connected processes to upload/download files
* Binary file transfer between processes are done via HTTP instead of local file movements to support deployments where these components don't necessarily share a common file system.

#### Publish supported configurations

The configuration types supported by a child device are expressed via a device provisioning message published to `tedge/meta` topic:

```json
{
    "config-types": ["data-mapping-config", "child-device-mapping-config"]
}
```

* The config list can also be requested explicitly by publishing a request to `tedge/commands/req/config/list` topic
* All processes that supports config management must respond to these requests by sending their config-list to `tedge/commands/res/config/list/<device_id>` topic
* The mapper then combines the config list published by different components and updates the same in the cloud

#### Pull config snapshot from device

* The mapper sends the config snapshot request from the cloud to the child device specific topic, with config type
  E.g: `tedge/commands/req/config/snapshot/<config_type>/<device_id>`
* The child device process listening on these snapshot requests acknowledges the receipt of this request by publishing an `executing` status to `tedge/commands/req/config/snapshot/<config_type>/<device_id>`
* The mapper, on receipt of this `executing` status forwards it to the connected cloud.
* The child device process then uploads the config snapshot to thin-edge via the file upload REST APIs exposed by `tedge-agent`.
  The URL for the uploaded binary is returned in the response by `tedge-agent`.
* The child device process marks the operation successful by sending the `successful` status to `tedge/commands/req/config/snapshot/<config_type>/<device_id>` with the binary URL received from `tedge-agent`
* The mapper on receipt of this successful message uses the URL in that response to download that file from `tedge-agent` and uploads it to the connected cloud and marks the operation successful in the cloud as well.

#### Push config update to device

* The mapper receives a config update request from the cloud with the config type and the config binary download URL
* The mapper then downloads the config binary from that download URL in the request
* The mapper then uploads this downloaded config file to thin-edge via the file upload REST APIs exposed by `tedge-agent`.
  The URL for the uploaded binary is returned in the response by `tedge-agent`.
* The mapper then sends the config update request to `tedge/commands/req/config/update/<config_type>/<device_id>` with the binary URL received from `tedge-agent` in the payload.
* The child device process acknowledges the receipt of this update request by publishing an `executing` status to `tedge/commands/req/config/update/<config_type>/<device_id>`
* The mapper, on receipt of this `executing` status forwards it to the connected cloud.
* The child device process then downloads the config file from `tedge-agent` using the URL in the request and applies it.
* The child device process marks the operation successful by sending the `successful` status to `tedge/commands/req/config/update/<config_type>/<device_id>`
* The mapper on receipt of this message marks the operation successful in the cloud as well.

### Proposal 2

<Chirtoph's file based config management proposal>
