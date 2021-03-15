# Architecture

Thin-edge.io is an open-source framework to develop lightweight, smart and secure connected devices.

Cloud agnostic, thin-edge.io provides a set of pre-packaged  modules
with plug & play connectors to cloud platforms,
device certificate management
as well as built-in software management.

Built around an extensible architecture,
thin-edge.io can be extended in various programming languages
to build telemetry applications with a combination of components


and device monitoring

analytics and machine learning components.

A mapper process can read a modbus source and emit translated data over MQTT.
While a 


Using processes 
The components must not have to designed with thin-edge.io in-mind to be able to used as a thin-edge.io componet

Some of these components will be designed for thin-edge.io.
 specifically for the 


![Overview](./thin-edge-overview.png)

* Use a [MQTT bus](./mqtt-bus.md) both for local communications and to connect the cloud.
* A canonical JSON format that the local processes can use to represent telemetry data. Cloud agnostic

