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

There are two options to create the cloud's child-device twin and declaring all supported capabilities:

1. The cloud's _child-device twin_ is created upfront (e.g. with some customer-specific cloud-site backend), and also all capabilities are declared that way to the new twin. Then the child-device on the device site (external or logical) relies on that existing twin.

2. The child-device initiates the device twin creation by announcing its identity and capabilities to thin-edge and then thin-edge gets that child device twin created in the cloud using a cloud mapper.
### (2) thin-edge APIs provided over the network<br/> and (3) associate consumed/provided data to child-device twins

Each thin-edge API that supports child-devices covers both aspects. As of now, the following APIs have child-device support:
   * Measurements, see [Sending a measurements to a child-devices](../tutorials/send-thin-edge-data.md#sending-measurements-to-child-devices)
   * Events, see [Sending an event to a child-device](../tutorials/send-events.md#sending-an-event-for-a-childexternal-device-to-the-cloud)
   * Alarms, see [Raising an alarm to a child-device](../tutorials/raise-alarm.md#raising-alarms-from-child-devices)
   * Configuration Management, see [Configuration files for child-devices](../references/c8y-configuration-management.md#configuration-files-for-child-devices)


