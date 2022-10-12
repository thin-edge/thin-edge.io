# Thin-edge Design Principles

__Thin-edge makes it easy the integration of cloud services, edge computing and operational technologies,
with the foundations to develop business-specific IIoT gateways
from a catalog of generic and use-case specific components assembled
on top of a framework that enables connectivity and interoperability.__

With IIoT, there is a fantastic opportunity for innovative use-cases and business models
that are more reactive, flexible, and efficient.
However, the challenges are many and each require a different expertise.
A typical IoT application has to address:

* __sophisticated business-cases__, with applications involving several vendors, actors and users
  and combining connectivity, device management, telemetry, monitoring, analytics;
* __very diverse integration needs__ over operating systems, protocols, data sources, processing tools,
  actuators, cloud end-points, legacy systems;
* __constraint environments__ with low-resources devices, high exposure to security risks,
  prohibitive manual operations, restricted upgrades;
* __business-critical or even safety-critical requirements__ with large fleet of devices
  operating in hazardous contexts and requesting long-term support.
  
Furthermore, the forces behind these requirements are pushing in opposite directions.

* On one side, to address the diversity of use-cases and integration requirements,
  one needs a large open set of features that can be easily extended and combined.
  [NodeRed](https://nodered.org/) is the archetype of such tools facilitating connections of components.
* On the other side, to meet resource and security constraints,
  one needs a minimal system specifically built for a task with cherry-picked components.
  Taken to the extreme, this leads to the idea of [unikernel](https://en.wikipedia.org/wiki/Unikernel)
  with sealed, single-purpose software images.
  
What makes thin-edge unique is its approach to reconcile these two poles
with two levels of building blocks that combine along an IIoT specific API.
* To maximize flexibility and interoperability,
  thin-edge provides the tools to build an agent from independent executables
  that interact over MQTT and HTTP using JSON messages.
* To minimize resources and vulnerabilities,
  thin-edge features fine-grain Rust components
  as well as the rules to combine them into hardened executables.
* To ease incremental developments,
  thin-edge give the developers the freedom to combine both MQTT and Rust-based components
  into purpose-specific gateways.
* To make feasible the interoperability of components provided by different tiers,
  thin-edge comes with an extensible data model for IIoT -
  cloud connectivity, telemetry data, device management, child devices, ...

## Design Principles

* Thin-edge provides the tools to develop an IIoT gateway by assembling components
  that have been implemented independently,
  possibly by different vendors and using different programming languages.
* There are two levels of components:
  * MQTT-based components that are processes running on the device and the local network
    and interacting over MQTT and HTTP using JSON messages.
  * Rust-based components that are actors running inside a process
    and interacting over in-memory channels using statically typed messages.
* These two levels of components serve different purposes and users:
  * The Rust-based components and their assemblage are designed for robustness.
    They follow strict combination rules enforced at compile time,
    notably with compatibility checks between message consumers and producers.
    These components provide the building blocks:
    * connections to IoT cloud end-points,
    * connections to various protocols (MQTT, HTTP, Modbus, OPC UA, ...),
    * interactions with the operating systems (files, commands, software updates, ...),
    * making sense of telemetry data (measurements, events, alarms, set points).
  * The MQTT-based components bring the flexibility to interact
    with external systems running on a different device or inside a container
    and without enforcing a programming language or an operating system.
    These components also provide the flexibility to experiment
    and to add missing features without having to implement a full-fledged Rust component. 
* To enable interactions between the MQTT-components, thin-edge provides an MQTT bus made of:
    * a local MQTT server,
    * an MQTT bridge that relays messages between the gateway and the cloud end-points,
    * an MQTT API that defines topics and message payloads
      * to exchange telemetry data,
      * to monitor components health,
      * to trigger operations and monitor progress,
    * a local HTTP server for operations where a REST API is more adapted than a Pub/Sub protocol.
* In practice, the software for an IIoT agent is build using a combination of MQTT and Rust components.
  * Thin-edge itself is released as an MQTT-based component built from Rust-based components.
  * A batteries included thin-edge executable is available to let the users experiment
    with all the available building-blocks.
  * An application-specific thin-edge can be built by cherry-picking building blocks
    and adding custom blocks for the business logic.
  * Along the main thin-edge executable, are deployed MQTT-based components over the local network,
    on external devices, controllers, PLCs, containers ... or even on the gateway
    connecting all the data sources, actuators, and data processors that make the application on the edge.
  
## Building an application with thin-edge

A thin-edge IoT application is built using two kinds of building blocks:
* At the system level, an application is built as *a dynamic assemblage of unix processes* that exchange JSON messages over an MQTT bus.
* Internally, a thin-edge executable is built as *a static assemblage of rust plugins* that exchange Rust-typed messages over in-memory channels. 

Thin-edge is shipped with general purpose executables, mappers and agent, aimed to ease on-boarding
with support for Cumulocity, Azure, collectd, external software-management plugins, thin-edge json, etc.
By using these main executables, an IoT-application developer can easily connect his device to the cloud
and other local tools like `apt`, `collectd` or `apama`.


```
                        #
┌────────────────┐      #      ┌─────────────────┐     ┌─────────────────┐
│                │      #      │                 │     │                 │
│  C8Y           │      #      │  Mapper         │     │  Agent          │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
└───────┬─▲──────┘      #      └──────┬─▲────────┘     └──────▲─┬────────┘
        │ │             #             │ │                     │ │
        │ │  SmartRest  #             │ │ JSON,CSV,SmartRest  │ │ JSON
        │ │             #             │ │                     │ │
┌───────▼─┴─────────────#─────────────▼─┴─────────────────────┴─▼────────────────┐
│  MQTT                 #                                                        │
│                       #                                                        │
└───────────────────────#─────────────▲───────────────────────▲──────────────────┘
                        #             │                       │
                        #             │ CSV                   │  JSON
                        #             │                       │
                        #      ┌──────┴──────────┐      ┌─────┴──────────┐
                        #      │                 │      │                │
                        #      │ Collectd        │      │ Third-party    │
                        #      │                 │      │                │
                        #      │                 │      │                │
                        #      │                 │      │                │
                        #      │                 │      │                │
                        #      │                 │      │                │
                        #      └─────────────────┘      └────────────────┘
                        #
                        #
```

If we zoom into a built-in thin-edge executable, then we have a different kind of components, Rust actors,
that exchange typed messages over in-memory channels.

For instance, the generic mapper provides support for Cumulocity, Azure, collectd
and telemetry data (measurements, events, alarms) collected over MQTT using the thin-edge JSON format.

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│ Generic Mapper                                           │
│                                                          │
│                                                          │
│  ┌─────────────────┐                                     │
│  │ c8y plugin      ├────► operations ──────┐             │
│  │                 │                       │             │
│  │                 │                       │             │
│  │                 ◄──┬─── telemetry ◄───┐ │             │
│  └─▲─┬─────────────┘  │     ▲            │ │             │
│    │ │                │     │            │ │             │
│    │ │                │     │            │ │             │
│    │ │  ┌─────────────▼───┐ │    ┌───────┴─▼───────┐     │
│    │ │  │ az plugin       │ │    │ thin-edge JSON  │     │
│    │ │  │                 │ │    │                 │     │
│    │ │  │                 │ │    │                 │     │
│    │ │  │                 │ │    │                 │     │
│    │ │  └──▲─┬────────────┘ │    └───────▲─┬───────┘     │
│    │ │     │ │              │            │ │             │
│    │ │     │ │              │            │ │             │
│    │ │     │ │     ┌────────┴────────┐   │ │             │
│    │ │     │ │     │ collectd plugin │   │ │             │
│    │ │     │ │     │                 │   │ │             │
│    │ │     │ │     │                 │   │ │             │
│    │ │     │ │     │                 │   │ │             │
│    │ │     │ │     └────────▲────────┘   │ │             │
│    │ │     │ │              │            │ │             │
│    │ │     │ │              │            │ │             │
│  ┌─┴─▼─────┴─▼──────────────┴────────────┴─▼─────────┐   │
│  │  MQTT Connection plugin                           │   │
│  │                                                   │   │
│  │                                                   │   │
│  └───────────────────────────────────────────────────┘   │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

The main motivation for this internal design is the ability to build specific executables that are smaller,
tuned for a specific use-case, consuming less memory and offering a reduced attack surface.

For instance, note that the generic mapper provides support for several clouds, even if the device will connect to only one.
Note also that MQTT is used to send operations to the agent via JSON over MQTT.
An application developer can easily reassemble cherry-picked thin-edge plugins into a highly tuned executable.

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│  tuned mapper + agent                                         │
│                                                               │
│    ┌────────────────┐                ┌─────────────────┐      │
│    │                │                │                 │      │
│    │ c8y plugin     ├─► operations───►  apt plugin     │      │
│    │                │                │                 │      │
│    │                │                │                 │      │
│    │                │                │                 │      │
│    └────▲─┬───▲─────┘                └─────────────────┘      │
│         │ │   │                                               │
│         │ │   │                      ┌─────────────────┐      │
│         │ │   │                      │                 │      │
│         │ │   └────────telemetry ◄───┤ thin-edge JSON  │      │
│         │ │                          │                 │      │
│         │ │                          │                 │      │
│         │ │                          │                 │      │
│         │ │                          └───────▲─────────┘      │
│         │ │                                  │                │
│         │ │                                  │                │
│         │ │                                  │                │
│   ┌─────┴─▼──────────────────────────────────┴────────────┐   │
│   │                                                       │   │
│   │  MQTT Connection plugin                               │   │
│   │                                                       │   │
│   │                                                       │   │
│   │                                                       │   │
│   └─────▲─┬──────────────────────────────────▲────────────┘   │
│         │ │                                  │                │
│         │ │                                  │                │
└─────────┼─┼──────────────────────────────────┼────────────────┘
          │ │                                  │
          │ │                                  │
          │ │                                  │
          │ ▼                                  │


          C8y                                 Sensors
```

The key points to be highlighted are that:
* To connect to thin-edge via MQTT, a component is not required to follow this design, not even to be written in Rust.
* The mapper and the agent provided by thin-edge out of the box can be used without any modifications.
* Building a tuned mapper or agent requires a Rust compiler but not a deep expertise in Rust.
  What has to be done is mostly to list the plugins to be included and to connect message producers and consumers.