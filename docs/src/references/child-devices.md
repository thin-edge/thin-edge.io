# Child-Device Support

Child-Device support allows to connect devices (e.g. sensors, actors, PLCs, any other kind of device) to the thin-edge device, to represent those devices in the cloud. Furthermore processes running on the thin-edge device can make use of child-device support to appear as a kind of a logical device in the cloud.

## Terminology
- **thin-edge device**:
  the physical device thin-edge is running on
- **external-device**:
  some kind of physical device that is connected to the _thin-edge device_ (e.g. a PLC, a fieldbus device, a sensor or any other kind of device)
- **child-device twin**:
  the entity in the cloud, that represents the child-device; in case of C8Y that is a managed object
- **child-id**:
  a string uniquely referencing the _external-device_ and the corresponding _child-device twin_
- **child-device agent**:
  that piece of software that makes the glue between the _external-device_ and thin-edge APIs; could be located on the _external-device_ or running as process on the _thin-edge device_

## Principles

Child-device support basically focuses on two aspects:
   1) enable external-devices to access thin-edge APIs via network
   2) equip thin-edge APIs to associate consumed/provided data with a child-device twin in the cloud

Each thin-edge API that supports child-devices applies to both aspects. As of now APIs as below have child-device support:
   * Measurements, see [Sending a measurements to a child-devices](../tutorials/send-thin-edge-data.md#sending-measurements-to-child-devices)
   * Events, see [Sending an event to a child-device](../tutorials/send-events.md#sending-an-event-for-a-childexternal-device-to-the-cloud)
   * Configuration Management, see [Configuration files for child devices](../references/c8y-configuration-management.md#configuration-files-for-child-devices)

## Child-Device Provisioning

The provisioning phase of a child-device includes:
  1) creating the child-device twin in the cloud
  2) declaring all supported capabilities on the child-device twin in the cloud
     
     (i.E. declaring all _supported operations_ and all _types_ per supported operation, as _config types_, _log types_, ...) 

The user journeys below outline the behaviour of external-devices, thin-edge and the cloud for the provisioning phase.

#### Personas used in user journeys
* **device-operator**: the one who maintains/operates the device in the field in the shopfloor
   
#### User Journey 1: Connecting an external-device as new child-device, provisioning managed by device
  - device-operator: connects new external-device to the thin-edge device by cable
  - device-operator: powers up the external-device
    - the external-device announces it-self to thin-edge with it's _child-id_ and capabilities (e.g. provided _configuration types_, _log types_, ...)
      Details, see: [_configuration types_](https://github.com/thin-edge/thin-edge.io/blob/cdab5683de9f9e0f34fa42a094ac399f6dbdc1e3/docs/src/references/c8y-configuration-management.md#managing-supported-configuration-list-of-child-devices)
    - thin-edge creates the child-device twin in the cloud, based on the _child-id_ reported by the external-evice

      NOTE: Cloud's operation to create the child-device twin is idempotent, so trying to create a child-device twin again that already exist does not harm.
    - thin-edge declares all capabilities reported by the external-device to the child-device twin (i.E. all _supported operations_ and corresponding types per operations, as _config-types_, _log-types_, ...)
  - from that point the new external-device is ready in operation

#### User Journey 2: Connecting an external-device as new child-device, provisioning managed by a cloud-site backend
  - device-operator: creates the child-device twin in the cloud using some customer-specific backend; as _child-id_ the device-operator uses some UID that is known by the external-device (e.g. the external device's serial number or network address)
  - device-operator: connects new external-device to the thin-edge device by cable
  - device-operator: powers up the external-device
    - the external-device announces it-self to thin-edge with it's _child-id_ and capabilities (e.g. provided _configuration types_, _log types_, ...)
    - thin-edge tries to created the child-device twin in the cloud, even though that twin was already created upfront.

      NOTE: Cloud's operation to create the child-device twin is idempotent, so trying to create a child-device twin that already exist does not harm.
    - thin-edge declares all capabilities reported by the external-device to the child-device twin (i.E. all _supported operations_ and corresponding types per operations, as _config-types_, _log-types_, ...)
  - from that point the new external-device is ready in operation

