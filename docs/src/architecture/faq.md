# Architecture FAQ

## Design principles
The primary goal of thin-edge.io is the ability to build IoT applications
around a large diversity of components provided by independent actors.

For that purpose, thin-edge.io aims to find the right balance between *interoperability*, *flexibility*, *security* and *efficiency*.

* __Interoperability__
  Integrate components producing or consuming telemetry data,
  northbound with cloud platforms, southbound with sensors
  as well as for east-west communications between analytics components.
* __Flexibility__
  Integrate component provided by different IoT actors,
  not even originally designed with thin-edge.io in-mind,
  using various technologies and programming languages.
* __Security__
  Provide secure and stable foundations for cloud connections, software updates, device connectivity.
* __Efficiency__
  Build applications that run on constrained device hardware and limited bandwidth networks.

## Why processes and not a library
Interoperability of software components can be addressed along very different approaches.
Thin-edge promotes dynamic and loose interactions over processes that exchange json text messages over a MQTT bus.

This offers great flexibility
when compared to a binary inter-process communication protocol with an interaction scheme enforced at compile time. 
Sure, the latter removes the overhead to serialize, transfer, validate and deserialize the messages.
But at the price of programming language constraints and foreign function interface (FFI) complexities.
In practice, a software component has to be specifically updated to interact with such a framework.

By contrast, with loose interactions of independent processes,
it becomes easier to integrate software component implemented by a tier,
and to implement adapter between the network, transport and application layers.
For instance, a mapper process can read messages from a Modbus gateway to publish translated messages over MQTT,
to be then read and translated by another independent mapper in the appropriate input for some analytics application.

Different contributors can then independently develop, package and combine software components
adding at will new data sources, processing capabilities and cloud endpoints. 

## Why MQTT

[MQTT](https://mqtt.org/) is a lightweight and flexible messaging protocol widely used by IoT applications.

* __Lightweight__
    * A MQTT client can run on heavily constrained device hardware and limited bandwidth networks.
* __Flexible__
    * The MQTT server ensures message delivery among clients but enforces neither a message format nor pre-organized message routes.
    * The components are free to create new message topics, to publish arbitrary messages over these topics,
      and to subscribe on topics in order to process any messages published there.
* __Widely used__
    * All the IoT cloud provides an MQTT endpoint to consume and publish messages from IoT devices.
    * Numerous IoT components feature MQTT capabilities to publish measurements and to listen to commands.
    * Protocol converters are available for most of the IoT protocols.


## Why JSON
Supported by any programming languages, JSON provides 
duck typing
flexible

## Why Rust
The command line interface, and the daemon processes of thin-edge.io are implemented in [Rust](https://www.rust-lang.org/),
*a language empowering everyone to build reliable and efficient software*.

Note beforehand that, even if the core of thin-edge.io is written in Rust,
any programming language can be used to implement thin-edge.io components.
For that, one just needs some support for MQTT and JSON.

So why Rust for the core of thin-edge.io?
programs are free from undefined behavior, data races or any memory safety issues. 

Security and stability on the key touch points with the cloud.
