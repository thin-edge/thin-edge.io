---
title: Architecture FAQ
tags: [Concept]
sidebar_position: 8
---

# Architecture FAQ

## Design principles
The primary goal of thin-edge.io is to simplify the connection of edge devices to the cloud
by providing a secure and reliable cloud connectivity as well as a device management agent.
The primary goal of thin-edge.io is the ability to build IoT applications
around a large diversity of components provided by independent actors.

For that purpose, thin-edge.io focuses on:

* __Interoperability__ -
  Thin-edge.io lets the users integrate components producing or consuming telemetry data,
  northbound with cloud platforms, southbound with sensors
  as well as for east-west communication between analytics components.
* __Flexibility__ -
  Thin-edge.io lets users integrate component provided by different IoT actors,
  not even originally designed with thin-edge.io in-mind,
  using various technologies and programming languages.
* __Security__ -
  Thin-edge.io provides a secure and stable foundation for cloud connections, software/firmware updates,
  and remote device management.
* __Reliability__ -
  Thin-edge.io components can survive in chaotic environments as network outages and process restarts happen.
* __Efficiency__ -
  Thin-edge.io lets users build applications that can run on constrained device hardware and with limited bandwidth networks.
* __Multi-cloud__ -
  Thin-edge.io enables users to connect their edge devices with multiple clouds.
  The actual cloud used can be decided at run-time by the end-user.

## Why is thin-edge.io an executable binary and not a library?
Interoperability of software components can be addressed along very different approaches.
Thin-edge.io uses dynamic and loose inter-process communication (IPC) using messages exchange over an MQTT bus.

In the past and even today, many clouds provide a library (SDK) to help you connect your code to the cloud.

In thin-edge.io we decided not to follow this approach because:
* Libraries are **programming language dependent**,
  and thus developing a library for a number of  programming languages always excludes developers using other
  programming languages. Additionally the effort to support many libraries (C, C++, Rust, Python, etc) is huge,
  including adding new features, testing, documentation, examples, Stack Overflow.
  Essentially we would create multiple small user groups instead of one large user group.
* Using an IPC mechanism (and not a library) makes it easier to **dynamically plug** together components during runtime
  (instead of recompiling the software). For example, it is easier to add additional protocol stacks
  (OPC/UA, modbus, ProfiNet, IO-Link, KNX, ...) to thin-edge.io during run-time. 
* Linking libraries to existing code can be problematic for some developers, for example for licensing reasons.
  While thin-edge.io has a very user-friendly licensing (Apache 2.0),
  some developers prefer to reduce the number of libraries that they link to their software.

## Why does thin-edge.io use MQTT for IPC?
[MQTT](https://mqtt.org/) is a lightweight and flexible messaging protocol widely used by IoT applications.

We were looking for a widely-used, performant IPC mechanism and we investigated a number of alternatives.
In the end, we decided to use MQTT for the following reasons:
* The approach is used by other industrial IoT organisations and software,
  for example by [Open Industry 4.0 Alliance](https://openindustry4.com/).
* Existing components (like [Node-RED](https://nodered.org/) or [collectd](https://collectd.org/) )
  that support MQTT can be easily integrated. In this case, thin-edge.io acts as an MQTT proxy:
  existing components connect to the local MQTT bus of thin-edge.io,
  and thin-edge.io routes the messages to different clouds in a secure and reliable manner.  
* MQTT is message oriented and bi-directional, which matches well with the event oriented programming model of industrial IoT.
* MQTT is available on many platforms, including Linux and Windows.
* MQTT client libraries are available for 25+ programming languages (see [MQTT.org](https://mqtt.org/software/)]) 
* MQTT overhead is relatively small in terms of client library size and network overhead.
* MQTT is message payload agnostic which enables sending not only JSON messages, but also text, CSV or binary data.  

Alternatives considered where: DBus, gRPC and REST over HTTP. 

## Why does thin-edge.io use MQTT for cloud communication?

[MQTT](https://mqtt.org/) is a lightweight and flexible messaging protocol widely used by IoT applications.
Nearly all the IoT cloud platforms provide an MQTT endpoint to consume and publish messages from a fleet of devices.
Therefore, MQTT was an obvious choice for edge to cloud communication.

Using MQTT for cloud communication is not mandatory. You are free to add additional protocols beside MQTT:
Because thin-edge.io has an internal bus, you can implement a bridge to another protocol (e.g. LWM2M or plain HTTPS).
In that case, MQTT is used inside the edge devices, and another protocol is used for external communication.

## Why is the thin-edge.io canonical format based on JSON?

[Thin-Edge-Json](./thin-edge-json.md), the cloud-agnostic message format of thin-edge.io, is based on JSON.

Supported by nearly all programming languages, JSON provides a nice compromise between simplicity and flexibility.
Notably, it features [duck typing](https://en.wikipedia.org/wiki/Duck_typing),
a flexible way to group different data fields that can be read
by consumers with different expectations over the message content.
For instance, a consumer expecting a temperature can process messages
where the temperature measurements are produced along with other kinds of measurements.

Additionally, JSON is supported by most (if not all) cloud vendors, which makes the transformation easier.

JSON is also used by other (Industrial) IoT standards, including OPC/UA and LWM2M.

## Why use Rust?
The command line interface, and the daemon processes of thin-edge.io are implemented in [Rust](https://www.rust-lang.org/),
*a language empowering everyone to build reliable and efficient software*.

The main motivation to use Rust is security: Rust avoids many security vulnerabilities and threading issues at compile time.
With the type system of Rust you write software that is free from typical security flaws:
undefined behavior, data races or any memory safety issues.

The second motivation is efficiency. Rust software is typically as efficient as C/C++ software. 
One reason is that Rust does not have (by default) a garbage collector. Instead, memory lifetime is calculated at compile time.

Note that, even if the core of thin-edge.io is written in Rust,
any programming language can be used to implement thin-edge.io components.
For that, one just needs an MQTT library that lets them interact with the thin-edge.io MQTT broker.
