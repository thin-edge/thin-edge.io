# Thin-edge Core Vision

A typical IoT application has to address:

* __sophisticated business-cases__, with applications involving several vendors, actors and users;
* __very diverse integration needs__ over operating systems, protocols, data sources, processing tools, actuators, cloud end-points;
* __constraint environments__ with low-resources devices, large-scale deployments, high exposure to security risks,
  prohibitive manual operations, long-term supports;
* __safety related requirements__ with devices operating machines in hazardous contexts.
  
The forces behind these requirements are pushing in opposite directions.

* On one side, from the use-case and integration perspective, one needs a dynamic and open setting with a large set of features that can be easily extended and combined.
  [NodeRed](https://nodered.org/) is the archetype of such tools facilitating connections of components.
* On the other side, to meet resource and security constraints, one needs a minimal system specifically built for a task with cherry-picked components.
  On the extreme side this leads to the idea of [unikernel](https://en.wikipedia.org/wiki/Unikernel) with sealed, single-purpose software images.
  
Can these two poles be reconciled?

The approach of thin-edge is to ease the development of IoT applications on edge devices with a smooth transition between:
* prototyping use-cases - when the aim is to easily onboard devices for IIoT,
* production use-cases - when the need is to deploy a hardened specific software on a large fleet of devices,
* as well as intermediate use-cases - when the application is build by several vendors with components written using different programming languages;
  or when a manufacturer assembles his tools on the devices while letting open to his customers the option to add their own modules. 

To achieve this goal, the foundations of thin-edge address the hardened case first.
* The core of thin-edge and its components are written using the *Rust* programming languages.
* The set of components used by an application is defined at *build time*.
* These components cooperate using a streaming, asynchronous message passing, *internal* API.
* Connections to the outside are delegated to specific bridge components
  that abstract the actual protocol (MQTT, HTTP, Modbus, OPC UA)
  while making these external channels accessible to the other components.
* The *internal* messages exchanged by the components are *predefined* and cover the domain of telemetry and IoT
  (measurements, events, alarms, operation requests, operation outcomes ...). 
  Messages sent to the outside are freely defined by the respective bridge components.
* The core of thin-edge provides the tools to configure the components as well as the internal message routes.
* An executable for an IoT application is statically defined by an assemblage of components and their configuration.
  
To open this static core, thin-edge provides bridge components opening channels to external end-points and message buses.
* Notably, thin-edge provides an MQTT component to connect external processes
  that are not necessarily part of the core, can be written using any programming languages
  and can run on other devices. 
* This MQTT bus works as an extension of the channels used internally by the core components.
  For that purpose, the internal thin-edge messages are serialized over MQTT using the thin-edge JSON schema.
* On top of this MQTT bus, thin-edge provides plugins to connect specific application,
  *e.g.* `collectd` or Apama.
* Most of the extensions of thin-edge will be provided by such bridge components:
  * to handle specific south-bound protocols as Modbus or OPC UA,
  * to trigger operations on the devices through a command line interface,
  * and, last but not least, to connect to specific cloud end-point with mapper components.


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