---
title: Typical IoT Agent
tags: [Concept]
sidebar_position: 1
---

# A typical IoT agent running thin-edge

## IoT agent

A typical IoT agent acts as a gateway between the cloud and devices deployed over machines and plants.

- Each gateway controls a piece of equipment, made up of devices with their hardware, software, sensors and actuators.
  From a cloud perspective, there are possibly tens of thousands of such pieces, each with its own gateway.
  However, here, the focus is on a single asset and the aim is to manage it from the cloud.
- The first requirement is to __manage__ from the cloud the __firmware__, __software packages__ and __configuration files__,
  which define the behavior of these embedded devices.
  As these are crucial for smooth running, one needs to control them at the scale of the fleet of assets,
  checking what is actually deployed and applying appropriate updates.
- The second requirement is to __collect__ __telemetry data__: __measurements__, __events__ and __alarms__
  from sensors, devices and running processes and to make these data available on the cloud
  and __monitor__ at scale the industrial processes supported by the equipment.
- The last but not the least requirement is to be able to __trigger operations__ from the cloud
  on the devices for maintenance or troubleshooting.

![Typical hardware](images/typical-iot-agent-hardware.svg)

## Connected to the cloud

All these capabilities are made available in the cloud using [__digital twins__](digital-twin.md),
which are virtual representations of the actual devices giving remote capabilities to:

- manage firmware, software packages and configuration files
- monitor the industrial processes
- operate the devices

## Running thin-edge

__The purpose of thin-edge is to support the development of such smart IoT agents__,
by providing the building blocks to:

- provide a uniform way to monitor and control miscellaneous hardware and software
  (to provide an abstract interface to integrate a diverse range of hardware and protocols)
- establish a digital twin (cloud representation) of each piece of equipment that needs to be remotely monitored and managed
- supervise operations triggered from the cloud
  for firmware, software and configuration management on these devices
- collect monitoring and telemetry data, forwarding these data to the cloud when appropriate

![Typical thin-edge deployment](images/typical-iot-agent.svg)

Thin-edge offers a combination of ready-to-use software components supporting the core features,
and extension points which allow users to develop small modular components
to meet specific requirements for the piece of equipment, hardware or application.

- An [__MQTT bus__](mqtt-bus.md) is used for all the interactions between these components.
  Thin-edge defines a [__JSON over MQTT API__](thin-edge-json.md) for the major features:
  telemetry data collection, service monitoring, remote operations
  as well as firmware, software and configuration management.
  To be precise, this API combines MQTT and HTTP,
  the latter being used for local file transfers and the former for asynchronous event processing.
- Thin-edge components, the [__agent__](tedge-agent.md) and a set of __operation specific plugins__, supervise all the operations,
  coordinating remote requests with the local thin-edge-compatible software components.
- Device-specific software components, the __child device connectors__, that interact with the hardware that make the piece of equipment.
  Note that the use of MQTT and HTTP APIs give the freedom to deploy these connectors directly on the associated hardware
  as well as on the main device acting as proxy, when, for some reasons,
  the child device software cannot be updated to directly support the thin-edge protocol.
- A cloud specific [__mapper__](tedge-mapper.md) handles the communication with the cloud,
  translating and forwarding requests and responses to and from the local components.
  This bidirectional communication establishes the twin live representation of the asset
  with its set of child-devices, services, configuration files and monitoring data.
