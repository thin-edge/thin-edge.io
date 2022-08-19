

## What is our vision?

The aim of creating thin-edge.io is to provide a IoT edge device framework for IoT project teams which makes it easy to enable resource constrained devices for IoT. Unlike other solutions, we are not just another single-purpose agent but a flexible platform with re-usable components without any vendor lock-in, focused on adressing the needs of both IT and OT users. 

## Our motivation

We believe that IoT (edge/device management, middleware, data analytics) are the driving forces of the fourth industrial revolution: companies are forced to leverage IoT solutions to stay competitive. In the past a large amount of companies have suffered from failed IoT projects and initiatives, where a lot of time and money was spent on device enablement: connectivity, security, device management, etc...

Thats why we created thin-edge.io. It provides an easy and flexible way for IoT project teams and device manufactures to make their devices IoT-ready without having to re-invent the wheel with costly and time-consuming, potentially embedded, development projects.

## Our target segments

**B2B (IoT) Service providers:**  IoT services providers are companies that are building and offering products and services based on IoT technology. For them it is crucial to deliver business outcomes and return on investment fast, as they are dealing with a lot of end-customers who themselves in the past often failed “homegrown” IoT initiatives. There is no willingness from the end customer to spend a large amount of money and time on device connectivity. Therefore thin-edge.io for them is a critical foundation to solve the connectivity challenge and to focus on the business applications rather then connectivity and device management aspects, which are in a lot of cases considered “hygiene factors”.

**(Smart) Equipment makers/Hardware manufacturers/OEMs:** Equipment Manufacturers are moving away from focusing primarily on selling their equipment towards selling their equipment as a service (EaaS). An IIoT platform allowing to connect & manage assets as well as to use the visualize, analyze, and integrate equipment data is often the foundation to enable service-based business models & services. Here thin-edge.io is used as a foundation to bring “intelligence” around the equipment. Combined with any IoT platform, single purpose gateways or devices which sit on the equipment can be transformed into edge deployment options for services and applications that support the overall EaaS business model by leveraging thin-egdge.io framework.

**Smart equipment operators:** Rather than dealing with single equipment, smart operators are looking to enhance or optimize whole manufacturing processes with the help of IIoT. Operating complex manufacturing systems requires not only to handle various industry protocols and assets but also requires to connect and manage a heterogeneous set of industrial devices and hardware such as PLCs, protocol converters or industrial gateways. Here thin-edge.io helps to unify the complex device landscape by offering lightweight software modules which can run on resource constrained, brownfield hardware.

## Our target users/personas

Within the different target segments, we are addressing the following personas with thin-edge.io:

**(IoT) Solution Developer/ Solution Architect**: Background consisting of Python, Java, JS, Angular, Kubernetes, Cloud Platforms
- Responsible for :
	- Implementing and maintaining the end-to-end IoT solution
	- Often juggling multiple initiatives covering a broad range of technology stacks in addition to implement and maintaining solutions.
- Challenges and needs regarding device enablement:
	- Lack of expertise and knowledge in embedded space
	- Dealing with fragmented hardware / linux variants
	- Lack of time to focus on device enablement as building IoT applications on cloud side is main responsibility
	- No interest/time to dive into “hygiene factors” as device management and security
	- Expect ready to use or configuration based solution, with pre-defined design principles and framework, offering easy extensibility  with known tools/languages 

**Device developer / Embedded engineer** with background in Linux, C/C++, C#, embedded systems (IT focused)

- Responsible for
	- device logic including firmware and software
	- primary only tasked to connect one or many devices/types to overall IoT solution
- Challenges and needs reagarding device enablement:
	- enable new services and connectivity on the device while keeping stability and robustness (while having limited computing resources)
	- dealing with certificates, queuing and persisting messages to handle instable connections, 
	- allowing the device to be managed centrally, to keep it secure and up-to-date (while important to him not always #1 prio to overall initiative/project)

- Special case : embedded dev with OT focus
	- familiar with PLCs, SCADA systems,
	- dealing with emerging need for connectivtiy and IIoT 
	- key concern is security, robusteness and resource efficiency which is usually overruling any “typical” IT solution on the device and implies some kind of custom logic. (e.g. rather closed OS, no dependencies can be installed, very strict certification and QA process , no CI/CD possible, long prodcut lifecycle 10-20 years)

**Summary:**

The persona types adressed by thin-edge.io often have conflicting requirements and views, this itself is addressed by the project technology vision and design principles, allowing thin-edge.io acting as a brindge between the OT and IT world. 

## What persona needs and problems do we address?

**Key problems for users:**

Until now, when developing device software, IoT proejct teams and device builders had to spend a lot of time solving generic challenges such as connecting to a cloud or IoT, dealing with certificates, queuing and persisting messages to handle instable connections or allowing the device to be managed centrally, to keep it secure and up-to-date.

Today, to overcome those challenges customers can either implement all those components themselves, which typical results in complex embedded code, which usually has less Individual, device-specific, custom code which needs to be maintained over the complete device lifecycle

Implementing all those functionalities on has very high complexity.  Furthermore, more and more companies require devices to be more flexible and host additional software which might be needed to offer new digital services to customers., while taking into account non-functional requirements such as low resource footprint, security and robustness.

The combination of those challenges often leads a series of custom developed embedded software that is expensive to maintain and extend. Also, most of the development used to be specific to one cloud or IoT platform. At the same time for more and more use-cases moving logic and analytics to the embedded edge device becomes a must for reasons such latency, security or cost. However, moving, and orchestrating workloads on the Edge used to be very challenging as it also requires a lot of custom logic to be developed on the device, to be able to integrate and support various device management platforms.

**Example applications scenarios:** 
Edge/embedded devices are critical compontent of any connected asset/smart equipment or operator use case. Within the different use-cases and application scenarios, edge can take over different roles to address different IoT challnges: 

-   Edge devices as machine gateway:
	- A typical problem for the target personas is the integration of  various asset specific OT interfaces to establish connection to fieldbus or industry protocols. 
- Supporting IoT and other Northbound connectivity 
	- Vendor agnostic connectivtiy of the data plane and control plane, analytics and service/app orchestration from various IoT platforms is required for all future IoT use-cases, as hyperscaler platforms and end-customer vendor preferences might vary. 
-   Edge devices as deployment option for IoT services and applications 
	- There is increasing need for device specifc applications close to the device e.g. device configuration, control logic, local monitoring, here a flexible software management framework is needed which is independent from the preferred sw artefact type due to different hardware, OS variants and package managers. 
-   There is an emerging needs for Edge analytics such as data filtering, pre-aggregation, ML model execution. 
-   Edge devices as configuration/management interface for asset - local/remote UI for asset configuration/management including software and firmware management of underlying systems to keep them secure and up-to date

## What is it and why thin-edge.io is a game changer?

To address the problems above, we created thin-edge.io as a framework for lightweight IoT gateways, PLC’s, routers and other embedded devices which require integration and interoperability with IoT platforms.

The framework includes modules for cloud connectivity, data mapping, device management, intra-edge communication, and certificate management, all aspects and challenges our target personas need to address.

In combination with IoT platforms, thin-edge.io is a foundation for enabling the devices with the following capabilities:

- support for effortless and secure edge device lifecycle management for single and device fleets
- support for low-touch provisioning of thin edge devices
- support for local and remote configuration
- support for local and remote maintenance including remote access (monitoring/troubleshooting),
- decommissioning of thin devices (e.g. for security compromosed or end of life devices)

Based on the challenge to capture both, the OT and IT persona needs, thin-edge.io is focused on following design principles:

- providing ready-to-use components available on wide-range of hardware, embedded linux varaints (thin edge layer)
- allowing control and orchestration from IoT (device management)  platforms
- effortless and secure edge software management for different software artifact types
- support effortless and secure edge analytics execution/management for different analytics artifact types and runtimes

**Why are we uniquely positioned to be competitive?**

thin-edge.io offers a unique approach to unify the needs of both the IT and OT world by offering a platform design focused on efficiency robustness and security while offering the highest level of extensibility and wide range of hardware.

- Compared to other frameworks we are not restricting users towards one specific software artifact type, package manager, programming language or message payload to be used on the device

- We combine robust and lightweight components with extensibility (plug-in mechanisms, mapper concept for cloud/platform support)

- We offer out-of-the-box modules to be used in combination with device management platforms

- We offer hardware and infrastructure agnostic deployment of all edge capabilities

## Additional Questions

What business goals do we try to achieve ?
- This is an open source project licenced under Apache 2.0 , it is focused on enterprise and company (B2B) use, hence why we encourage further partners to join our mission and community to create an industry standard for IoT device enablement and vendor agnostic connectivity and device management. 
