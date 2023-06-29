---
title: MQTT Bus
tags: [Concept, MQTT]
sidebar_position: 3
---

# Thin Edge MQTT bus

Thin-edge uses a combination of MQTT and HTTP to coordinate the interactions
between the miscellaneous hardware and software components that make up a piece of equipment.

- A local MQTT broker is deployed along each thin-edge enabled piece of equipment.
- Each device and service is given dedicated MQTT topics to publish their measurements, events and alarms
  as well as to receive targeted operation requests.
- Similarly, a local HTTP server is used to transfer files to and from child devices,
  using a URL schema that is aligned with the schema used to specifically target devices over MQTT. 

Collectively, the MQTT broker and the normalized MQTT topic schema form the __MQTT bus__.
The local HTTP server and the normalized URLs schema are referred as the __file transfer service__.

## Local Digital Twin

The MQTT topic schema establishes the local representation of a piece of equipment, with its main device, child devices and services.
This local representation has a one-to-one relationship with the remote digital twin.
However, while the latter is a user interface, the former is akin to a program interface, an API.

- the main device as well as the child devices are each represented by a topic
- each type of telemetry data, measurements, events, alarms, is given a specific sub-topic of the topic related to the source device
- each type of operation is given a specific sub-topic under the target device topic

## Cloud Bridges

The MQTT bus is extended to connect the cloud using an MQTT bridge
that routes messages back and forth between the local MQTT broker and the remote MQTT endpoint.

Such a bridge is cloud specific by definition - each kind of message having to be sent or received from specific topics.

## Local File Transfer

Using MQTT is appropriate to collect telemetry data from devices and services
as well as to trigger operations on devices and services.
However, it's more pertinent to use HTTP to transfer files from one device to another.
And this is not only because of payload size constraints, but also for *pull* versus *push* constraints.

By combining MQTT with HTTP, a thin-edge service can *notify* a child-device that a file is *available* for local transfer
and then let the child-device *download* the file when *ready*.

A notable use is to install a new version of firmware, software package or configuration file on a child-device.
The device management service will start the update operation by making the associated file locally available,
before notifying the child-device of the update request
and simply monitoring the progress of the operation once delegated to the child-device.
