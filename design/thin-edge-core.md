# Thin-edge Design Principles

__Thin-edge makes it easy the integration of cloud services, edge computing and operational technologies,
with the foundations to develop business-specific IIoT agents
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
  into purpose-specific agents.
* To make feasible the interoperability of components provided by different tiers,
  thin-edge comes with an extensible data model for IIoT -
  cloud connectivity, telemetry data, device management, child devices, ...

## Design Principles

* Thin-edge provides the tools to develop an IIoT agent by assembling components
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
    * an MQTT bridge that relays messages between the agent and the cloud end-points,
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

Thin-edge is shipped with a batteries-included executable, the `tedge` command, that eases on-boarding
with support for various IoT clouds, monitoring tools, software-management plugins, OT protocols ...
On top of `tedge`, an IoT-application developer can easily build a purpose-specific IIoT agent
to connect his devices to the cloud and local resources. 
This IIoT agent is made of independent processes that interact over MQTT using JSON messages,
and that are deployed over the devices on the edge as well as the OT network.
The IoT-application developer can implement and deploy his own components,
using his programming language of choice,
and leveraging the thin-edge MQTT API to interact with the components of the agent.

```
                        #
┌────────────────┐      #      ┌─────────────────┐     ┌─────────────────┐
│                │      #      │                 │     │                 │
│  C8Y           │      #      │  tedge          ├────►│  apt            │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
│                │      #      │                 │     │                 │
└───────┬─▲──────┘      #      └──────┬─▲────────┘     └─────────────────┘
        │ │             #             │ │ 
        │ │  SmartRest  #             │ │ JSON,CSV,SmartRest
        │ │             #             │ │ 
┌───────▼─┴─────────────#─────────────▼─┴────────────────────────────────────────┐
│  MQTT                 #                                                        │
│                       #                                                        │
└───────────────────────#─────────────▲───────────────────────▲──────────────────┘
                        #             │                       │
                        #             │ CSV                   │  JSON
                        #             │                       │
                        #      ┌──────┴──────────┐      ┌─────┴──────────┐
                        #      │                 │      │                │
                        #      │ Collectd        │      │ OT device      │
                        #      │                 │      │ - IPC          │
                        #      │                 │      │ - PLC          │
                        #      │                 │      │ - μ controller │
                        #      │                 │      │                │
                        #      │                 │      │                │
                        #      └─────────────────┘      └────────────────┘
                        #
                        #
```

This design provides the flexibility to interact with various sub-systems on the edge,
to experiment easily and to address application specific needs. 
However, this also lead to a system that is too heavy and more fragile than expected for most use cases,
with unused features embarked by the batteries-included `tedge`,
and numerous independent components to operate consistently.

Hence, the need for fine-grain components. These are provided as rust components.
Internally, a thin-edge executable is built as *a static assemblage of rust actors*
that exchange statically-typed messages over in-memory channels.

Each actor provides a very specific feature that most of the time doesn't make sense in isolation,
but only in combination with the other actors. For instance,
* The `collectd` actor role is to translate messages received from `collectd`
  into `Measurement` Rust values that can be then consumed by any other actor
  that accepts this type of data, as the `c8y` actor does.
* Similarly, the `c8y` actor acts as a translator between Cumulocity IoT and the other actors.
  It consumes `Measurement` Rust values and translates them into `MQTTMessage` for Cumulocity IoT.
  In the reverse direction, the `c8y` actor consumes `MQTTMessage` from Cumulocity IoT,
  and produces `Operations` encoded as Rust values and ready to be consumed by other actors.
* The `az` actor has a similar role, except that the translations are done for Azure IoT.  
* Neither the `collectd` actor nor the `c8y` actor have to handle an MQTT connection.
  They simply produce and consume, in-memory representations for `MQTTMessage`
  that are sent over the wire by the `MQTT` actor, accordingly to the MQTT protocol.
* Connected to each others in a batteries-included `tedge` executable,
  these actors provide support for Cumulocity, Azure, collectd
  and telemetry data (measurements, events, alarms).
* Among all the actor, one play a key role. This is the `thin-edge JSON` actor.
  This actor materializes the thin-edge MQTT API,
  defining topics and message payloads that are exchanged by the MQTT-bases components,
  and translating these messages into Rust-values ready to be consumed by the other actors.
  The `thin-edge JSON` actor is the interface between the two levels of thin-edge components.

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│ batteries-included `tedge`                               │
│                                                          │
│                                                          │
│  ┌─────────────────┐                                     │
│  │ c8y actor       ├────► operations ──────┐             │
│  │                 │                       │             │
│  │                 │                       │             │
│  │                 ◄──┬─── telemetry ◄───┐ │             │
│  └─▲─┬─────────────┘  │     ▲            │ │             │
│    │ │                │     │            │ │             │
│    │ │                │     │            │ │             │
│    │ │  ┌─────────────▼───┐ │    ┌───────┴─▼───────┐     │
│    │ │  │ az actor        │ │    │ thin-edge JSON  │     │
│    │ │  │                 │ │    │     actor       │     │
│    │ │  │                 │ │    │                 │     │
│    │ │  │                 │ │    │                 │     │
│    │ │  └──▲─┬────────────┘ │    └───────▲─┬───────┘     │
│    │ │     │ │              │            │ │             │
│    │ │     │ │              │            │ │             │
│    │ │     │ │     ┌────────┴────────┐   │ │             │
│    │ │     │ │     │ collectd actor  │   │ │             │
│    │ │     │ │     │                 │   │ │             │
│    │ │     │ │     │                 │   │ │             │
│    │ │     │ │     │                 │   │ │             │
│    │ │     │ │     └────────▲────────┘   │ │             │
│    │ │     │ │              │            │ │             │
│    │ │     │ │              │            │ │             │
│  ┌─┴─▼─────┴─▼──────────────┴────────────┴─▼─────────┐   │
│  │  MQTT Connection actor                            │   │
│  │                                                   │   │
│  │                                                   │   │
│  └───────────────────────────────────────────────────┘   │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

The main motivation for this internal design is the ability to build specific executables that are smaller,
tuned for a specific use-case, consuming less memory and offering a reduced attack surface.

An application developer can easily reassemble cherry-picked thin-edge actors into a highly tuned executable,
keeping only the feature actually required on the IIoT agents,
and possibly adding Rust actors that have been implemented specification for his application.

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│  tuned `tedge`                                                │
│                                                               │
│    ┌────────────────┐                ┌─────────────────┐      │
│    │                │                │                 │      │
│    │ c8y actor      ├─► operations───►  apt actor      │      │
│    │                │                │                 │      │
│    │                │                │                 │      │
│    │                │                │                 │      │
│    └────▲─┬───▲─────┘                └─────────────────┘      │
│         │ │   │                                               │
│         │ │   │                      ┌─────────────────┐      │
│         │ │   │                      │                 │      │
│         │ │   └────────telemetry ◄───┤ thin-edge JSON  │      │
│         │ │                          │    actor        │      │
│         │ │                          │                 │      │
│         │ │                          │                 │      │
│         │ │                          └───────▲─────────┘      │
│         │ │                                  │                │
│         │ │                                  │                │
│         │ │                                  │                │
│   ┌─────┴─▼──────────────────────────────────┴────────────┐   │
│   │                                                       │   │
│   │  MQTT Connection actor                                │   │
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
* An MQTT-bases component is not required to follow this design, not even to be written in Rust.
* The executables provided by thin-edge out of the box are MQTT-based components,
  built as an assemblage of Rust actors. They can be used without any modifications.
* Building a tuned thin-edge executable requires a Rust compiler but not a deep expertise in Rust.
  What has to be done is mostly to list the actors to be included and to connect message producers and consumers.

## What kind of component for my use case?

What are the pros & cons of Rust-based and MQTT-based components?
When use one or the other?

| MQTT-based thin-edge executable                                                | Rust-based thin-edge actor                                              |
|--------------------------------------------------------------------------------|-------------------------------------------------------------------------|
| Coarse grain component used to build IIoT agents                               | Fine grain component used to build MQTT-based thin-edge executables     |
| Process running on the local network                                           | Actor running inside a process                                          |
| Interact over MQTT and HTTP using JSON messages                                | Interact over in-memory channels using statically typed messages        |
| Can be written in any language                                                 | Must be written in Rust                                                 |
| Deployed over hosts and containers on the edge local network                   | Integrated into an executable in combination with other actors          |
| Required to enable interactions over several devices and cloud end-points      | Leverage a thin-edge-json actor to interact with MQTT-based executables |                        
| Facilitate prototyping and interactivity                                       | Build for robustness and frugality                                      |
| Might lead to a resource greedy agent with too many daemon processes           | Help to optimize resource utilisation within a single process           |
| Might lead to deployment and dependency issues with loosely-coupled components | Dependencies and compatibilities are checked at compile-time            |
| The `tedge` command is an MQTT-based executable                                | The `tedge` command is an assemblage of Rust-based thin-edge actors     | 
