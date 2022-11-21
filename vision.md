# What is our vision?

The aim of creating thin-edge.io is to make it easy for IoT device integrators to enable resource constrained devices for IoT. Unlike other solutions, we are not just another single-purpose agent but a flexible framework with re-usable components without any vendor lock-in, focused on addressing the needs of both IT and OT users.

## Our motivation

IoT integration of cloud and edge computing is a fantastic opportunity for innovative use-cases and business models - more reactive, flexible, and efficient. However, the challenges are many, demanding and each require a different expertise: connectivity, security, device management, telemetry, analytics...

Thats why we are creating thin-edge.io. It provides an easy and flexible way for
IoT project teams and device manufactures to make their devices IoT-ready
without having to re-invent the wheel with costly and time-consuming,
potentially embedded, development projects and without having to reevaluate
existing projects or applications to enable them with thin-edge.io.

## Example usage 

While the thin-edge.io framework addresses a broad range of customer segments, it is primarily used in industrial or manufacturing environments by companies that can be characterized as: 
 
-       Industrial gateway or IoT hardware manufacturers (e.g. industrial routers, PLCs or embedded Linux devices) 
-       IIoT service providers (IoT or industrial automation software companies) 
-       Industrial equipment makers and operators (machine builders, OEMs)
 
For them thin-edge.io can be used as a foundation to build new digital services and products. 
 
More specifically thin-edge.io focuses on addressing the overlapping needs and challenges around:
1.     device connectivity (e.g. connect industrial sensors and automation equipment to a cloud service) 
2.     device management (e.g. monitoring the health of the device or keeping software, firmware, and configurations up to date via a central device management platform)
3.     device customization (e.g. extending the capabilities of the existing gateway with additional services and applications with thin-edge.io integrated with optional custom-developed components)
 
thin-edge.io helps to unify the complex device landscape by offering lightweight software modules which can run on resource-constrained, brownfield hardware. Furthermore, it is providing a frictionless user experience to enable customers to develop their own solutions based on the included functionality but also by attaching custom functionality with minimum effort. The high level of robustness, security, and scalability allows going from a POC implementation with thin-edge.io to a rollout in production with little friction.

## Users and Personas

thin-edge.io targets the following user types: 

- "End User"  
    - Person using a binary that has been built with the thin-edge.io framework
      as well as a finished and working configuration
    - Does not interact directly with the device but only through an IoT / Cloud / Platform UI 
    - Does not modify or extend the device directly (e.g. through the extension mechanisms of thin-edge.io)
    - Typically has no embedded development background/knowledge 
    - An example of an end user is a fleet operator of a centrally managed, large device fleet of elevators, industrial machines, or similar, responsible for monitoring, maintenance, and troubleshooting of those assets via a device management platform.

- "System Integrator"
    - Person using a binary that has been built with the thin-edge.io framework
      and crafts a configuration suited to solve their problems
    - Has direct access to the device and interacts with the configuration options of thin-edge.io to influence the device behavior 
    - Might have basic embedded programming knowledge but prefer configurations
    - Requires an easy way of interacting and re-configuring thin-edge.io (e.g. via CLI, scripts, or config templates)
    - An example for a system integrator user type is a field/automation engineer or factory IT admin who is tasked to re-configure a device so it can send telemetry to an IoT platform or receive OTA software updates. 

- "Interface Developer"
    - Person using the foreign interfaces of a thin-edge.io framework binary to
      complement and extend its functionality.
An example for an "Interface Developer" is a **(IoT)
Solution Developer/ Solution Architect**: Background consisting of Python, Java,
JS, Angular, Kubernetes, Cloud Platforms:

- Responsible for implementing and maintaining the end-to-end IoT
  solution including the needed logic on the device (aka device agent)
- Challenges and needs regarding device enablement:
    - Lack of expertise and knowledge in embedded space, therefore prefers configurations or extensions in python
    - Dealing with fragmented hardware / linux variants
    - Focused on expertise outside of embedded device enablement such as analytics, sensor integration or dashboarding 
    - Expect ready to use or configuration-based solutions, with
      pre-defined design principles and framework, offering easy
      extensibility with known tools/languages
- "Plugin Developer"
    - Person using the provided Rust crates to extend or improve binary plugins
      written for the thin-edge.io framework
An example for "Plugin Developer" is a **Device developer / Embedded engineer** whose background consists of Linux, C/C++, C#, embedded systems (IT focused):

- Responsible for
    - device logic including firmware and software
    - primarily only tasked to connect one or many devices/types to overall IoT
      solution
- Challenges and needs regarding device enablement:
    - enable new services and connectivity on the device while keeping stability
      and robustness (while having limited computing resources)
    - dealing with certificates, queuing, and persisting messages to handle
      unstable connections
    - allowing the device to be managed centrally, to keep it secure and
      up-to-date (while important to him not always #1 prio to overall
      initiative/project)

Also an "Embedded Developer" with OT focus is an example of a "Plugin Developer":

- familiar with PLCs, SCADA systems,
- dealing with the emerging need for connectivity and IIoT
- key concern is security, robustness, and resource efficiency which is
  usually overruling any "typical" IT solution on the device and implies
  some kind of custom logic. (e.g. rather closed OS, no dependencies can be
  installed, very strict certification and QA process, no CI/CD possible,
  long product lifecycle 10-20 years)
- "Core Developer"
    - Person writing on the provided Rust crates as hosted on github.com



The users addressed by thin-edge.io often have conflicting
requirements and views, this itself is addressed by the project technology
vision and design principles, allowing thin-edge.io acting as a bridge between
the OT and IT world.

## Pain points addressed by thin-edge.io 

**Key problems for users:** Until now, when developing device software, IoT
project teams and device builders were spending a lot of time solving generic
challenges such as connecting to a cloud or IoT, dealing with certificates,
queuing and persisting messages to handle unstable connections or allowing the
device to be managed centrally, to keep it secure and up-to-date.

To overcome these challenges today, customers can either implement all those
components themselves, which typical results in complex embedded as well as
individual, device-specific code which needs to be maintained over the complete
device lifecycle.

Implementing these functionalities oneself has not only a very high complexity
but also an opportunity cost, as those resources could have been used for more
specific business needs. On top of that, companies wish to be more and more
flexible so as to stay relevant in an ever-changing digital landscape.

Of course, any generic solution would have to walk the very tight path of
keeping a low resource footprint, strong security and robustness.

The combination of those challenges often leads a series of custom developed
embedded software that is expensive to maintain and extend. Also, most of the
development used to be specific to one cloud or IoT platform. At the same time
for more and more use-cases moving logic and analytics to the embedded edge
device becomes a must for reasons such latency, security or cost. However,
moving, and orchestrating workloads on the Edge used to be very challenging as
it also requires a lot of custom logic to be developed on the device, to be able
to integrate and support various device management platforms.


## Benefits and key capabilities

To address the problems above, we create thin-edge.io as a framework for
lightweight IoT gateways, PLC’s, routers and other embedded devices which
require integration and interoperability with IoT platforms.

The framework includes modules for cloud connectivity, data mapping, device
management, intra-edge communication, and certificate management, all aspects
and challenges our target personas need to address.

We therefore define the following requirements.


### Capabilities

<!--
The following list of capabilities should be considered non-final. It might be
extended in the future.
-->
We believe that device enablement for the major cloud providers in the IIoT space should not only be easy and secure but also easily extensible for various IoT services to allow a best-of-breed combination of IoT capabilities from multiple platforms/services. 
 
Therefore thin-edge.io focused on providing: 
 
- An out-of-the-box integration with IoT and cloud platforms as well as an extensible connectivity framework to create additional cloud/IIoT platform connectors with state-of-the-art security. 
A comprehensive set of vendor-agnostic device management capabilities that can be easily extended and integrated with various IoT and device management platforms. Here our focus is to enable the complete edge device lifecycle management functionalities in typical IIoT applications such as:
o   low-touch provisioning of thin edge devices and its child/downstream devices 
o   Local and remote configuration of the device and its services 
o   Remote access for device monitoring and troubleshooting
o   Decommissioning of thin devices (e.g. for security compromised or end-of-life devices)
o   Local and over-the-air software/package/file and firmware management including OS updates, configuration updates (of both the thin-edge.io deployment as well as other software) as well as firmware updates for "southbound"/child devices
An foundation for easy integration of existing "South bound" connectivity software and drivers to support major protocols in the IIoT industry. The intention of this is to primarily enable existing protocol libraries to be easily integrated with other thin-edge.io components and the overarching cloud/IIoT platform. 
A simplified, IoT-centric domain and messaging model, allowing a vendor-agnostic representation of devices  and telemetry coming from sensors, actors and machine assets with language-agnostic integration mechanisms to persist data in local databases or to allow pre-processing and data filtering provided by other services on the device.

## Closing words

thin-edge.io offers a unique approach to unify the needs of both the IT and OT
world by offering a platform design focused on efficiency robustness and
security while offering the highest level of extensibility.

- We are not restricting users towards one specific software artifact type,
  package manager, programming language or message payload to be used on the
  device
- We combine robust and lightweight components with extensibility (plug-in
  mechanisms, mapper concept for cloud/platform support)
- We offer out-of-the-box modules to be used in combination with device
  management platforms
- We offer hardware and infrastructure agnostic deployment of all edge
  capabilities

