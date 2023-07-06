---
title: Concepts
tags: [Concept]
sidebar_position: 2
---

import DocCardList from '@theme/DocCardList';

This section covers the concepts of thin-edge,
starting with the problem to solve
and presenting the building blocks used by thin-edge to build flexible solutions.

In order to __build [IoT Agents](typical-iot-agent.md) connected to cloud [Digital Twins](digital-twin.md)__,
thin-edge provides a set of versatile software components: 

- The [MQTT Bus](mqtt-bus.md) to interconnect the entities making up a device: software components and hardware. 
- [Thin Edge JSON](thin-edge-json.md) to be standardize the communication over the MQTT Bus.
- The [Agent](tedge-agent.md) to implement the device management features.
- The [Mapper](tedge-mapper.md) to connect a device to the cloud. 
- [Extension Points](software-management.md) to extend thin-edge to specific application domains, operating systems or hardware. 


<DocCardList />
