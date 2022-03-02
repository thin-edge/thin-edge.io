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