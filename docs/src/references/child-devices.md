# Child-Device Support

Child-Device support allows to connect devices (e.g. sensors, actors, PLCs, any other kind of device) to the thin-edge device, to represent those devices in the cloud. Furthermore processes running on the thin-edge device can make use of child-device support to appear as a kind of a logical device in the cloud.

## Terminology
- **thin-edge device**:
  the physical device thin-edge is running on
- **external-device**:
  some kind of physical device that is connected to the _thin-edge device_ (e.g. a PLC, a fieldbus device, a sensor or any other kind of device)
- **logical child-device**:
  a process running on the _thin-edge device_ that makes use of thin-edge's child-device API to be represented as individual device in the cloud
- **child-device**:
  a _logical child-device_ or an _external-device_ that makes use of thin-edge's child-device API to be represented as individual device in the cloud
- **child-device twin**:
  the entity in the cloud, that represents the child-device; in case of C8Y that is a managed object
- **child-id**:
  a unique string referencing the _external-device_ or _logical child-device_ to the corresponding _child-device twin_
- **child-device agent**:
  that piece of software that makes the glue between the _external-device_ and thin-edge APIs; could be located on the _external-device_ or running as a process on the _thin-edge device_ itself

## Principles

Child-device support basically focuses on three aspects:
   1) provision the child-device, i.e. making thin-edge and the cloud aware of the child-device (external or logical) 
   2) thin-egde APIs provided on network, to enable external-devices APIs to access them
   3) associate consumed/provided data of a child-device (external or logical) to it's child-device twin in the cloud

### (1) Child-Device Provisioning

A newly attached child-device (external or logical) must be once provisioned. The provisioning phase of a child-device includes:
  1) creating cloud's child-device twin
  2) declaring all supported capabilities of the child-device to its child-device twin
     (i.e. declaring all _supported operations_ and all _types_ per supported operation, as _config types_, _log types_, ...)
     (i.e. declaring all operations supported by the device like configuration management, software management etc and further metadata per supported operation, like _config types_, _log types_, _software list_ etc)

There are two option to create the cloud's child-device twin and declaring all supported capabilities. 

On the one hand a cloud's _child-device twin_ can be created upfront (e.g. with some customer-specific cloud-site backend), and also all capabilities can be declared that way to the new twin. Then the child-device on the device site (external or logical) relies on that existing twin.

2. The child-device initiates the device twin creation by announcing its identity and capabilities to thin-edge and then thin-edge gets that child device twin created in the cloud using a cloud mapper.

##### Announcing child-device capabilities by the child-device it-self

To announce it's capabilities the child-device (external or logical) sends an MQTT message to `tedge/meta/<childid>`. The MQTT message is as below:

```json
{
   "device-name": "<optional name of that child-device>",
   "device-type": "<optional type of that child-device>",
   "<capability 1>": <capability specific JSON object>,
   "<capability 2>": <capability specific JSON object>,
   // [...]
}
```

Thereby the fields are as below:
   * `device-name` is an optional human readble device-name, visible in the cloud. If the field is not contained in the JSON message the `childid` of the topic structure is used as device-name.
   * `device-type` is an optional device-type string assigned to the cloud's child-device twin. If the field is not contained in the JSON message the value `thin-edge.io-child` is used as device-type string.
   * each field `<capability i>` represents a capability (e.g. `configurations` for _Configuration management_). The format of each `capability specific JSON object` is specific to the coresponding capability. For details see the documentation for coresponding capabilitie's API (e.g. [Configuration files for child-devices](../references/c8y-configuration-management.md#configuration-files-for-child-devices)). 
     Section [thin-egde APIs provided on network](#2-thin-egde-apis-provided-on-network-and-3-associate-consumedprovided-to-child-device-twins) below lists all APIs provided by thin-edge for child-device support.

Example:
```json
{
   "device-name": "My Child-Device",
   "device-type": "type1",
   "configurations": [ // capabilities for feature Configuration Management 
     {
       "type": "foo.conf"
     },
     {
       "type": "bar.conf"
     }
   ]
}
```

Whenever that MQTT message is sent to the thin-edge device:
  1) thin-edge assures the child-device twin exists, and creates it if it does not exist.
  2) each software component that provides an included capability (e.g. the C8Y Configuration Plugin for `configurations`) takes care to define all necessary capabilities to the coresponding cloud's child-device twin (i.E. _supported operations_, and list of provided types as e.g. _configuration types_).

In response to the child-device provisioning request, thin-edge publishes an empty resposne to MQTT topic `tedge/meta/success/<child-id>` on success, or a failure message to `tedge/meta/failed/<child-id>` in case of a failure. The failure message on `tedge/meta/failed/<childid>` can be optional.

### (2) thin-edge APIs provided over the network<br/> and (3) associate consumed/provided data to child-device twins

Each thin-edge API that supports child-devices covers both aspects. As of now, the following APIs have child-device support:
   * Measurements, see [Sending a measurements to a child-devices](../tutorials/send-thin-edge-data.md#sending-measurements-to-child-devices)
   * Events, see [Sending an event to a child-device](../tutorials/send-events.md#sending-an-event-for-a-childexternal-device-to-the-cloud)
   * Configuration Management, see [Configuration files for child-devices](../references/c8y-configuration-management.md#configuration-files-for-child-devices)


## User Journeys

The user journeys below outline the behaviour of external-devices, thin-edge and the cloud during the provisioning phase.

#### Personas used in user journeys
* **device-operator**: the one who maintains/operates the device in the field in the shopfloor
   
#### User Journey 1: Connecting an external-device as new child-device, provisioning managed by device
  - device-operator: connects new external-device to the thin-edge device by cable
  - device-operator: powers up the external-device
    - the external-device announces it-self to thin-edge with it's _child-id_ and capabilities (e.g. provided _configuration types_, _log types_, ...)
      Details, see: [Child-Device provisioning](#1-child-device-provisioning) above
    - thin-edge creates the child-device twin in the cloud, based on the _child-id_ reported by the external-evice

      NOTE: Cloud's operation to create the child-device twin is idempotent, so trying to create a child-device twin again that already exist does not harm.
    - thin-edge declares all capabilities reported by the external-device to the child-device twin (i.E. all _supported operations_ and corresponding types per operations, as _config-types_, _log-types_, ...)
  - from that point the new external-device is ready in operation

#### User Journey 2: Connecting an external-device as new child-device, provisioning managed by a cloud-site backend
  - device-operator: creates the child-device twin in the cloud using some customer-specific backend; as _child-id_ the device-operator uses some UID that is known by the external-device (e.g. the external-device's serial number or network address)
  - device-operator: declares all capabilities to the child-device twin (i.E. all _supported operations_ and corresponding types per operations, as _config-types_, _log-types_, ...)
  - device-operator: connects new external-device to the thin-edge device by cable
  - device-operator: powers up the external-device
    - the external-device announces it-self to thin-edge with it's _child-id_
    - thin-edge tries to created the child-device twin in the cloud, even though that twin was already created upfront

      NOTE: Cloud's operation to create the child-device twin is idempotent, so trying to create a child-device twin that already exist does not harm.

    - thin-edge retrieves all declared capabilities from the child-device twin (e.g. provided _configuration types_, _log types_, ...) and send as response to the external-device

  - from that point the new external-device is ready in operation

