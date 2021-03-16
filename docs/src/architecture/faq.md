# Architecture FAQ

## Design principles
The primary goal of thin-edge.io is the ability to build IoT applications
around a large diversity of components provided by independent actors.

For that purpose, thin-edge.io aims to find the right balance between *interoperability*, *flexibility*, *security* and *efficiency*.

* __Interoperability__ -
  Thin-edge.io let integrate components producing or consuming telemetry data,
  northbound with cloud platforms, southbound with sensors
  as well as for east-west communications between analytics components.
* __Flexibility__ -
  Thin-edge.io let integrate component provided by different IoT actors,
  not even originally designed with thin-edge.io in-mind,
  using various technologies and programming languages.
* __Security__ -
  Thin-edge.io provide secure and stable foundations for cloud connections, software updates, device connectivity.
* __Efficiency__ -
  Thin-edge.io let build applications that run on constrained device hardware and limited bandwidth networks.

## Why processes and not a library
Interoperability of software components can be addressed along very different approaches.
Thin-edge promotes dynamic and loose interactions over processes that exchange json text messages over a MQTT bus.

This offers great flexibility
when compared to a binary inter-process communication protocol with an interaction scheme enforced at compile time. 
Sure, the latter removes the overhead to serialize, transfer, validate and deserialize the messages.
But at the price of programming language constraints and foreign function interface (FFI) complexities.
In practice, a software component has to be specifically updated to interact with such a framework.

By contrast, with loose interactions of independent processes,
it becomes easier to integrate software components implemented by tiers,
and to implement adapter between the network, transport and application layers.
For instance, a mapper process can read messages from a Modbus gateway to publish translated messages over MQTT,
to be then read and translated by another independent mapper in the appropriate input for some analytics application.

Different contributors can then independently develop, package and combine software components
adding at will new data sources, processing capabilities and cloud endpoints. 

## Why MQTT

[MQTT](https://mqtt.org/) is a lightweight and flexible messaging protocol widely used by IoT applications.

Designed to run on heavily constrained device hardware, MQTT is a de-facto standard for IoT.
Most the IoT products feature MQTT capabilities, either directly or indirectly via protocol converters.
Similarly, all the IoT cloud provides an MQTT endpoint to consume and publish messages from a fleet of devices.

MQTT perfectly fits the requirements for flexibility of thin-edge.io.
A MQTT server ensures message delivery among clients but enforces neither a message format nor pre-organized message routes.
The clients are free to create a mesh of message topics and to exchange arbitrary messages over these topics.

Thin-edge.io [MQTT bus](./mqtt-bus.md) proposes - but doesn't enforce - an organisation of topics
that can be leverage as a basis for inter-process communication among software components running on the device.
This includes topics replicated to the cloud in a bridge mode
as well as topics with an associated [mapper](./mapper.md) translating measurements
sent in a [cloud agnostic file format](./thin-edge-json.md) into the format actually expected by the cloud the device is connected to.

## Why JSON

[Thin-Edge-Json](./thin-edge-json.md), the cloud-agnostic message format of thin-edge.io, is designed as a subset of JSON.

Supported by any programming languages, JSON provides a nice compromise between simplicity and flexibility.
Notably, it features [duck typing](https://en.wikipedia.org/wiki/Duck_typing),
a flexible way to group different data fields that can be read
by consumers with different expectations over the message content.
For instance, a consumer expecting a temperature can process messages
where the temperature measurements are produced along other kind of measurements.

## Why Rust
The command line interface, and the daemon processes of thin-edge.io are implemented in [Rust](https://www.rust-lang.org/),
*a language empowering everyone to build reliable and efficient software*.

The main motivation is security and stability, two key attributes for critical services
interacting with the cloud and enabling features like device connection, remote control and software update.
The type system of Rust enable the implementation of programs 
that are free from the kind of stability issues usually exploited as security flaws:
undefined behavior, data races or any memory safety issues.

The second motivation is efficiency.
With no runtime nor garbage collector, Rust programs are efficient and can be tuned to run on embedded devices.

Note that, even if the core of thin-edge.io is written in Rust,
any programming language can be used to implement thin-edge.io components.
For that, one just needs some support for MQTT and JSON.
