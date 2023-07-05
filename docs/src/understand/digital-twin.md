---
title: Digital Twin
tags: [Concept]
sidebar_position: 2
---

# Digital Twin

Thin-edge provides the tools to build
a remote *digital twin* representation of a piece equipment
as well as a *local representation* of its architecture,
both being used to direct operations to and collect data from
the key components of the equipment, being hardware, software or data.

This digital representation gives an actionable view to the remote user
as well as to the device operator and to local processes.

- A cloud operator uses the digital twin to monitor, operate and manage the device,
  visualizing the telemetry data collected from the device components
  and triggering operations on these remote components from their user interface.
- On the piece of equipment itself, the architecture is represented by an MQTT topic schema,
  with topics specifically attached to the key components
  from which data is collected and to which operation requests are forwarded.

It's important to note that a digital twin is a user's perspective
that might differ from the physical architecture of the device.
Indeed, the hardware components might have specificities irrelevant for the operators,
and these technical details are abstracted away by the digital twin.

## Building Blocks

Thin-edge represents a piece of equipment as a hierarchy of devices and services,
with a main device, nested child-devices and the services running on these devices.

### Device

A device represents a hardware component, of which the firmware, software packages and configuration files are managed from the cloud.
This is also a target of maintenance operations and a source of telemetry data.

A device doesn't always have to be a physical device, but can also be a logical one,
abstracting some parts or groups of parts of the equipment.

### Service

A service represents a piece of software running on a device that is monitored from the cloud.
It can be an application, an operating-system process, an industrial process or a thin-edge daemon.

From a digital twin perspective, a service is essentially a source of monitoring data;
but could also be a target of maintenance operations such as configuration updates and restarts.

### Main Device

The main device represents the piece of equipment as a whole.
This is usually, but not necessarily, the gateway device where thin-edge is running.

### Child Device

A child device is a device that is attached to the equipment.
A child device can have further nested child devices as well,
forming a hierarchy, with the main device as root and the services as leafs,
zooming in from the piece of equipment into its parts.

### Device Profile

A device profile groups what defines the behavior of a device:
its __firmware__, __software packages__ and __configuration files__.

The actual profile of a device is represented in its digital twin
from where it can be managed, checked and updated.

There are no strict definitions of the different kind of profile parts.
For instance, some AI model can be seen as a software package or a configuration file.
The differences are more related to tooling and update constraints.
The firmware cannot be removed and might require a device restart on update.
A configuration file can be edited by an operator,
while a software package is usually downloaded from a repository.

### Telemetry Data

Telemetry data encompasses __measurements__, __events__ and __alarms__
that are collected, generated or raised by sensors and processes.

These data are propagated by thin-edge from the physical devices to their digital twins;
and made available to the cloud application, users and operators.

:::note
The term "telemetry data" is also used to
represent metrics and monitoring data related to the operating system and running processes.
:::

### Operations

Conversely, maintenance operations are triggered from the cloud and forwarded by thin-edge to the targeted devices.

It can be to restart a device, to update a configuration file, or to install software packages,
using a precise workflow, checking prerequisites, coordinating participants and monitoring progress.

## Design

The digital twin of a piece of equipment is designed to support end-users and operators.
This is an *operational representation* of the main parts of the equipment,
which is used to identify the source of telemetry data and the target of maintenance operations,
with the appropriate level of details from the application perspective:

- details are abstracted away by the digital twin
- the piece of equipment is represented by a hierarchy of nested devices
- each device defines its own set of capabilities: what can be managed and monitored
- capabilities are implemented by thin-edge or by device connectors

### Hardware Abstraction

The digital twin of a piece of equipment is an abstraction:

- some parts of the equipment might be omitted, others grouped
- sensors are represented as source of measurements attached to a device or a service
- actuators are controlled indirectly using operations targeting a device

In fact, the device representation is actually just software acting as proxy for the physical hardware.
This software implements the abstraction, publishing telemetry data and handling operation requests.

Similar to a device representation, a service is free to define what it should represent.
It can be an operation process, an application, some thin-edge components, or some other custom entity.
From a digital twin perspective a service is a source of telemetry data that is running on a device.

### Nested Devices

If most low-level details are abstracted away,
it can still be important to give to the operator a conceptual view of the main equipment parts.
This is done using a hierarchy of nested devices,
from a main device to more and more specific child-devices,
up to services running on those devices.

To implement this nested view, thin-edge provides the tools to
- uniquely identify each device and service making up an asset
- attach metadata on the type and role of each part
- define how these devices are nested

### Capabilities

Each device can define its own set of capabilities.
Depending on the device and its software, the users will be able to:
- observe telemetry data collected by the device and the services running on it
- collect log files from the device and its services
- manage, check and update configuration files
- manage, list and update software packages installed on the device
- update the firmware of the device
- trigger application operations

These capabilities are declared to thin-edge using a combination of convention and configuration.
They are then reflected on the digital twin and made available to the local and remote end-users.

### Device Connectors

The capabilities of a device are implemented by __device connectors__,
i.e. software components that use the thin-edge API
to publish telemetry data on behalf of the device and to handle operation requests
acting on the hardware, its profile and the services running under the hood.

Thin-edge itself is a device connector.
Installing thin-edge on a device enables the capabilities provided by thin-edge out-of-the-box, on that device.

A device connector is typically installed on the connected device itself. However, this is not a rule.
A device connector can run on different hardware
which can be useful when the software of target device cannot be updated.

## Example

![Device Concept](images/device-concept.svg)




