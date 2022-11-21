# What is our vision?

The aim of creating thin-edge.io is to make it easy for IoT device integrators to enable resource constrained devices for IoT. Unlike other solutions, we are not just another single-purpose agent but a flexible framework with re-usable components without any vendor lock-in, focused on addressing the needs of both IT and OT users.

## Our motivation

IoT integration of cloud and edge computing is a fantastic opportunity for innovative use-cases and business models - more reactive, flexible, and efficient. However, the challenges are many, demanding and each require a different expertise: connectivity, security, device management, telemetry, analytics...

Thats why we are creating thin-edge.io. It provides an easy and flexible way for
IoT project teams and device manufactures to make their devices IoT-ready
without having to re-invent the wheel with costly and time-consuming,
potentially embedded, development projects and without having to reevaluate
existing projects or applications to enable them with thin-edge.io.

## Our target segments

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


An example for an "Interface Developer" or a "Plugin Developer" is a **Device
developer / Embedded engineer** whos background consisting of Linux, C/C++, C#,
embedded systems (IT focused):

- Responsible for
    - device logic including firmware and software
    - primarily only tasked to connect one or many devices/types to overall IoT
      solution
- Challenges and needs reagarding device enablement:
    - enable new services and connectivity on the device while keeping stability
      and robustness (while having limited computing resources)
    - dealing with certificates, queuing and persisting messages to handle
      unstable connections
    - allowing the device to be managed centrally, to keep it secure and
      up-to-date (while important to him not always #1 prio to overall
      initiative/project)

An "Embedded Developer" with OT focus is an example of a "Plugin Developer":

- familiar with PLCs, SCADA systems,
- dealing with emerging need for connectivity and IIoT
- key concern is security, robusteness and resource efficiency which is
  usually overruling any "typical" IT solution on the device and implies
  some kind of custom logic. (e.g. rather closed OS, no dependencies can be
  installed, very strict certification and QA process, no CI/CD possible,
  long prodcut lifecycle 10-20 years)

The persona types adressed by thin-edge.io often have conflicting
requirements and views, this itself is addressed by the project technology
vision and design principles, allowing thin-edge.io acting as a bridge between
the OT and IT world.

## Persona needs and solutions

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

### Non-functional requirements

<!--
The following list of non-functional requirements should be considered
non-final. It might be extended in the future.
-->

1. Multi-paradigm edge framework
    - Several kinds of users are be able to consume thin-edge.io at different
      stages (e.g. using the Rust code as a framework to develop their own
      solutions, using (networking)interfaces like for example MQTT to attach
      their own infrastructure to thin-edge.io, or using the final
      deployment-ready binary to solve their problems).
1. Introspective functionality to enable quality assurance as well as support of
   deployments
    - Knowing what happens inside a system is crucial to be able to quickly
      identify misbehaving components. Users are be able to access an overview
      of the communication of the different components
1. Ability to attach data sources and data sinks to thin-edge.io via different
   mechanisms, either by implementing connectors directly in the framework using
   rust code, attaching them via some networking mechanism (e.g. MQTT) or other
   mechanisms the framework provides
    1. The framework ships with connectivity functionality for the most common
       connectivity requirements
    1. Configuration of connectivity functionality is exposed to the user
    1. Connectivity over the same mechanism but with different configuration
       settings is easily doable for the user (e.g. using MQTT to connect to
       different brokers with completely different settings)
1. Easy deployment and little friction when using thin-edge.io as a framework to
   develop their own solutions based on it
1. Trust in the user to do the right thing with the tools provided, respect
   their decisions and guide them towards a maintainable solution by design
    - Users are assumed to be experts in their domain and should be given the
      tools they need to solve the problems they have. Unnecessarily burdening
      them with abstractions only causes frustration
1. High configurability and tweakability with high trust in custom changes
   without having to redefine the world on small changes
1. Allow precise configuration inside common solutions shipped by the framework
    - Users are be able to change a small detail of their deployment without
      having to then define unrelated elements
    - As an example: Users wanting to set an SSL certificate should not need to
      worry about also defining their DNS setup.
1. Reproducability of a configuration
1. Security
    1. Operational Security
        1. Components of the framework are encapsulated and do not influence
           each others execution
        1. The system can recover from bad states in a safe way
        1. Untrusted input is handled in a way that does not influence execution
           in a bad way (e.g. crashes the system)
    1. Information Security
        1. Data access is reglemented
        1. Data origin is verified and potentially encrypted
    1. Code Audit(ability)
        1. Functionality can be audited easily because it is encapsulated (see
           above)
1. Generic solutions over specific solutions
    - Problems are solved in a way that users can define the details as best
      suited
1. Easy configurability and high discoverability of configuration options
    - Configuring the system is not done across dozens of configuration files at
      different locations, and instead be centralized and easy to write
    - Configuration options are easily discoverable and documented, for example,
      in a highly secured environment, the user is not required to access some
      online documentation ressource, but is able to access all configuration
      documentation via the binary shipped or derived from the thin-edge.io
      framework
    - Configurations are representable in multiple ways, e.g. as text or in a
      more visual format (graph, image overview)
1. The user os able to enable and disable functionality of the framework
    - Not all functionalities of the framework are required in every use-case,
      users are able to disable or even remove those parts from deployed
      binaries
    - For example: if only approved and audited components are allowed to be
      used
1. Deriving specialized implementations/binaries from the framework is possible
    - The thin-edge.io ships a potentially large all-in-one binary with as much
      functionality as the project provides, but
    - Tailoring down the binary and removing unused functionality is easily
      doable with minimal effort by a System Integrator
    - Building a specialized binary with additional custom functionality is
      easily doable by an Plugin Developer
1. As little overhead as possible
    1. in CPU time: The Framework ensures that the absolute minimum of CPU time
       is consumed to deliver the desired functionality the user configured the
       framework to do
    1. in Memory usage: The Framework ensures that as little as possible memory
       is in use, at every point in execution time, as possible
1. Compatibility
    - Configuration and components are written in such a way that they may be
      changed in the future with no or minimal impact on their intended purpose

### Functional requirements

<!--
The following list of functional requirements should be considered
non-final. It might be extended in the future.
-->

1. The framework provides MQTT connectivity through a component
1. All errors must be handled in a non-crashing way
    - Unrecoverable errors may still cause the binary to shutdown eventually,
      but not unexpectedly.
1. The core implementation is written in Rust
    - Plugins that connect to a thin-edge.io binary, may be written in another
      programming language
1. A deployment of thin-edge.io consists of a single binary with a single
   configuration entry-point
    - The final configuration may consists of more than one resource (file,
      network resource, etc), and potentially be even loaded over the network,
      this is left open
1. The configuration is the single point of truth w.r.t. the initial state of
   the components mentioned within
    1. The default state of a plugin must be documented
    1. A component may use a (documented) default value for a missing
       configuration entry
1. The configuration is the single point of truth for the communication between
   components inside a single thin-edge.io binary deployment
1. The communication between the components is verified to be compatible in
   advance
1. The out- and in-bound connectivity is mediated through a framework specific
   format
    - JSON is the lingua franca, but other forms may be acceptable if they stay
      within the capabilites of JSON
    - json-schema is used to document the (JSON) format in a machine-readable
      way
1. The principal way of extending thin-edge.io is over well-defined Rust interfaces (traits and other types)
    - This does not preclude other forms of extensions in other languages (i.e.
      a bridge over MQTT to python)
1. Starting a thin-edge.io binary for development use or production use is not
   interactive
    - Starting a thin-edge.io binary for setup or similar purposes may be
      interactive
1. Components can persist data using the framework
    - to persist data between restarts of the deployment
    - to cache data during network outages
    - to provide operation checkpoints during sensitive operations
1. User-configurable logging is provided via the framework
    - A user is able to configure logging per-component as well as globally for
      the framework

### Capabilities

<!--
The following list of capabilities should be considered non-final. It might be
extended in the future.
-->

In combination with IoT platforms, thin-edge.io is a foundation for enabling
devices with the following capabilities:

1. Connectivity to the major cloud providers in the IIOT space
    - for effortless and secure edge device lifecycle management for single
      and device fleets
    - for low-touch provisioning of thin edge devices
    - for local and remote configuration
    - for local and remote maintenance including remote access
      (monitoring/troubleshooting)
    - decommissioning of thin devices (e.g. for security compromised or end of
      life devices)
1. "South bound" connectivity to devices via the major protocols in the IIOT industry
1. On-device data preprocessing
    1. Analytical (timeseries analysis, ML, etc...)
    2. Mathematical operations (avg, sum, etc...)
    3. Logical operations
1. On-device data generation (e.g. generating of events)
1. Device management functionality
    1. OS updates
    1. Package updates
    1. Configuration updates (Of both the thin-edge.io deployment as well as other software)
    1. Firmware updates for "south bound"/child devices
1. Device interaction
    1. File upload/download
    1. Filesystem listings
    1. General system informations
1. Persist data as given by components in different databases (e.g. PostgreSQL, Sqlite, MongoDB, Redis, Memcached, ...)
1. Documentation of each component and their configuration
    - This includes information on how to configure each configurable aspect of the component and its valid states
    - This also includes all message types that the framework knows about
    - Users with custom thin-edge.io deployments must be able to generate such a documentation themselves

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

