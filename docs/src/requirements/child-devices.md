# Child Devices

## Child Device Model
* A child device implies two aspects:
  - an _endpoint at the device side_, that represents the child-device; that could be a pysical device that is connected to the thin-edge device (e.g. a PLC, a fieldbus device, a sensor or any other kind of device);
   or a container or local process running on the thin-edge device
  - a _cloud child device-twin_ that represents the child-device in the cloud
  - ?an edge child device-twin (a logical representation of the physical device in thin-edge)
* Physical child devices typically are special-purpose hardware devices designed to do just a single task or a handful of tasks at best. 
* These devices can't (or don't want, out of security reasons) connect to the cloud directly. Instead they get connected to the cloud via a gateway device.
* The gateway device runs thin-edge, and all physical child devices are connected to the gateway device via network, fieldbus or any other channel. 
* The gateway device appears in the cloud with it's digital device-twin, with one digital device-twin per each physical child-device underneath.
* The relation between a physical child-device and the according cloud device-twin is made with a child-id.
* The gateway takes care of routing all data/requests/responses beetween the physical child-devices and the related cloud child-device twins.
* Multiple physical child devices could be connected to a single gateway.

TODO: do not forget, a child-device could be also logical (a process running on the gateway)

# Initialisation of a Child-Device
Before a child-device could be put into action, both aspects from above (_Device-Side Endpoint_ and _Cloud Child Device-Twin_) need to be initialised.

* Initialisation: Device-Side Endpoint

   ...TODO...

* Initialisation: Cloud Child Device-Twin

  Two levels of initialisations have to take place for the cloud child device-twin: 
  1) creating the new child-device twin in the cloud
  2) declaring capabilities on the new child-device twin, i.E.:
     - declaring supported operations (e.g. sw-update, config-man, log-man, ...)
     - declaring supported types (e.g. sw module-type, config-type, log-type, ...)

# User Journey

## Definition of Personas used in the user journeys below
* __Device Operator__ (the one who maintains/operates the device in the field in the shopfloor)
* __Agent Developer__ (the one who adapts and deploys thin-edge to the device)
* __Device App Developer__ (the one who implements the device application that makes use of thin-edge APIs)
* __Solution Architect__ (the one, who is in authority for the whole end-to-end picture (from devices to cloud) )


## Journey: Connecting a physical child-device to the cloud via the gateway

Preface: There are two options to create a child-device twin in the cloud:
  1) by cloud site or by some customer specific tool/app on-top of the cloud
  2) by the thin-edge device<br/>
     a) automatically no 1st data push<br/>
     b) intentionally by configuration or external device request

Same two options are possible for declaring capabilities.

Scenarios below are per each combination of those options:

* __Scenario 1: Child Device-Twin created by Cloud, Child-Device Capabilties declared by thin-edge__
  1. Device Operator creates the child-device twin in the cloud, with a unique child-id.
  2. Device Operator plugs the physical child-device to the gateway.
  3. Device Operator configures physical child-device's capabilities on the gateway (e.g. config management items, ...).
  4. Device Operator installs any needed device-specific sw components that translates the thin-edge protocol into the device-specific protocol (e.g. SW Man Plugins, ...).
  5. Device Operator configures address and credentials/certificate of the gateway to the pyhiscal child-device (e.g. broker IP/Port/Certificate).
  6. Device Operator configures unique child-id of the cloud's child device-twin in the physical child device.
  7. Device Operator validates if all capabilities of the child-device are working.
     TODO: How? -> Logfiles of thin-edge(?). Needed to try everything by hand(CfgMan,...)???
  TODO: In case of any error use thin-edge logs to identify and solve the problem.

* __Scenario 2: Child Device-Twin created by thin-edge, Child-Device Capabilties declared by thin-edge__
  
  ...TODO...
  
* __Scenario 3: Child Device-Twin created by Cloud, Child-Device Capabilties declared by Cloud__
  
  ...TODO...

* __Scenario 4: Child Device-Twin created by thin-edge, Child-Device Capabilties declared by Cloud__
  
  ...TODO...

## Connecting a non-smart / closed physical child-device to the cloud via the gateway
TBD

## Sending telemetry data from physical child-device to its cloud child-device twin
TBD 

## Sending operations from the cloud child-device twin to the physical child-device
TBD

These devices might generate some telemetry data based on the actions that they perform.
They'll typically have some factory-installed apps, which can be configured to specific customer environments using configurations.
Even though these apps can be reconfigured to adjust their functionality, updating these apps themselves with newer versions are rarely done.
Installing newer applications almost never happens as these devices generally aren't general purpose compute devices that can run random apps.
Some devices just supports a single-purpose app that's usually labelled as the firmware of the device.

To connect such devices to an IoT cloud platform, thin-edge needs to enable the following:

* Life cycle
  - To connect a physical child device with its device twin that already exists in the cloud
  - ? To create the device twin in the connected cloud with any metadata describing the capabilities/properties of the physical device
  - To update the existing metadata of the device twin in the cloud, reflecting the changes in the physical device(apps updated, configs installed etc)
* Telemetry support
  - To receive any structured non-binary data generated by the device and route it to its device twin in the cloud
* Remote command APIs
  - To route control commands/instructions coming from the cloud device to the physical device conncted to it, like restart, reset or other custom controls
  - Install/update an app, app config or firmware on the device remotely from the cloud
  - Update set-points or properties of the device (e.g: desired speed of a rotor, paint color of a robot) to represent a desired change in the functioning of the device
* Binary payload APIs
  - To exchange configuration, app, firmware or any other binaries with the connected cloud

Thin-edge needs to provide these, without expecting those devices to have a thin-edge installation directly in them.
It just needs to act as a gateway that provides these devices with access to resources in the cloud, without connecting directly,
by just routing any data received from the device to the cloud and vice-versa.
When binaries are involved, thin-edge must make these binaries available on the gateway, from which child devices can easily access them.
Since child devices are often connected to the gateway over a network, all data exchanges must happen over the network.

### Mapping to Cumulocity data model

For the features described in the previous section, Cumulocity provides the following APIs over both MQTT(S) and HTTP(S).

* Lifecycle APIs
  - [Managed Object APIs](https://cumulocity.com/api/10.13.0/#tag/Managed-objects) to create and update device twins
  - Device's metadata and operation capabilities are described as JSON fragments
* Telemetry APIs
  - [Measurements](https://cumulocity.com/api/10.13.0/#tag/Measurements): for numeric data
  - [Events](https://cumulocity.com/api/10.13.0/#tag/Events): for non-numeric data representing "real-time events" happening in a system
  - [Alarms](https://cumulocity.com/api/10.13.0/#tag/Alarms): for alerts requiring manual action
* Remote command APIs
  - Device control: To control the device remotely, like restarting the device
  - Firmware: To manage the main app/firmware on the device
  - Software: To manage apps of different kinds when a device supports more than one app
  - Configuration: To manage the configuration of an existing app
  - [Custom operations](https://cumulocity.com/api/10.13.0/#tag/Operations):To exchange any customized device control commands(e.g: set points)
* Binary APIs[https://cumulocity.com/api/10.13.0/#tag/Application-binaries]
  - Repositories for configs, software and firmware

## Assumptions

1) A child device is a physical device (a PLC or a sensor or a smart device like IMX-500) connected to the thin-edge device (typically a gateway device) over the network (io-link, fieldbus or even over IP).
2) Generally there are two kinds of child devices:
   - a) A dumb device that is connected to the thin-edge device via an IO-Link or fieldbus, that can't run the processing logic that sends/receives data to/from the thin-edge device. For such devices, the processing logic will be running on the thin-edge device itself.
   - b) A smart device that is connected to the thin-edge device over an IP network, with the processing logic running on that device itself. Its interactions with the thin-edge device will only be over the network
3) In both kinds of devices, the process that handles all data exchanges to/from the physical child device is called the "child device process"
4) Thin-edge's responsibility is to receive and route any cloud-bound data from these child device processes and any device-bound data like software/config binaries to these child device processes.
5) The child device process itself is resposible for pushing these binaries to the physical device itself and applying it there.
6) Each child device has a unique ID that can be used to identify/distinguish between them

TODO: Add somehow statement below as definition: 
"PLCs are already long time in production-line and no additional software can be installed (e.g. MQTT client) or configuration changed, to make the PLC capbable to communicate with the thin-edge device. That is to avoid any risc in the production line. Instead the thin-edge device must use existing interfaces from the PLC."

## Questions

1) Are child devices and their unique IDs static and known at thin-edge provisioning time, or do they dynamically get connected to thin-edge?
2) How are device twins provisioned? Should thin-edge provide some APIs or is it always done directly in the cloud?
3) Should thin-edge provide routing of data between child devices (from one child device to another)?
4) Will there be one "child device process" per "physical child device"? Or a single process can manage multiple physical child devices at once?
5) Should thin-edge also support receiving data from multiple child devices and combining all that data and associate it with the main thin-edge device twin itself in the cloud(instead of having multiple child devices)? Customer typically do this as a cost-cutting measure with clouds charging fees per device.
6) Can a child device have further hierarchical child devices of its own (grandchild devices of thin-edge)?

## Use-Cases

* **Use-Case 1: Gathering telemetry data from connected PLCs and handle as Measurements, Alarms or Events**
   - a process on the thin-edge device collects process-data from a CODESYS PLC via local network (using the CODESYS PLC handler)
   - per each connected PLC a process is running
   - the process pushes gathered PLC's process-data to thin-edge measurements, alarms or events
   - the process has a configuration file that contains mapping information, meaning which process-data to push to which thin-edge measurement, alarm or event;
     and which child-device ID to use
   

* **Use-Case 2: Viewing/applying data-mapping configuration of PLCs using Configuration Management**
   - the process' configuration (see process configuration file in use-case 1) is managed by Configuration Management
   - each PLC appears as child-device in the Cloud, that supports individual Configuration Management functionality per child-device
   - thereby data mapping can be changed or extended from cloud side

* **Use-Case 3: Update application on connected PLCs using Software Management**
   - each PLC appears as child-device in the Cloud, that supports individual Software Management functionality    
   - a commandline tool on the thin-edge device is capable to send a CODESYS application (one file) to a connected PLC, and start it
   - on an incoming SW Management update request for a PLC that commandline tool is invoked to install the CODESYS application on the according PLC


NOTE 1: For use-cases above the external device could be also another PLC than CODESYS or even another device than a PLC.

NOTE 2: Use-case(s) with external device that can directly access thin-edge interfaces (e.g. tedge/measurements) not yet covered.

# Requirements

- **Requirement 1:** Thin-edge has to provide child-device support for interfaces: <br/>
     - Measurements/Alarm/Events<br/>
     - Configuration Managements<br/>
     - Software Managements Plugin API<br/>

- **Requirement 2:** Where thin-edge provides child-device support it uses corresponding Clouds' child device meachnism, to associate data/functionality of that child-device as own device underneath the thin-edge device.

- **Requirement 3:** When data is pushed to thin-edge for a child-device, thin-edge will take care that the child-device exists in the Cloud, and creates it if needed.

- **Requirement 4:** Thin-edge is stateless in terms of child devices. I.e. it does not store a list of child-devices of even a state of those. All knowledge about child-devices is in the applications that provide/consume data and functionality from/to child-devices. 

- Requrirement 5: thin-edge fetches the Configuration from the device and pushed to the cloud
    - Thin-edge should deliver the config upload request from the cloud to the appropriate child device process
    - Expose an API with which the child device process can upload the snapshot of the config file binary to the thin-edge device.
    - Thin-edge should then upload and associate these config files with their corresponding device twin in the cloud.
    - Thin-edge must make sure that config files uploaded by one child device should not overwrite the config uploaded by another child, even if they are of the same type

- Requrirement 6: thin-edge downloads configuration file from the cloud and delivers the configuration to the child-device process
    - Thin-edge should deliver the "apply config request" along with any assiciated binaries from the cloud to the appropriate child device process
    - Applying the config file is the responsibility of the child device process itself
    - Thin-edge must make sure that a config file downlaoded for one child device does not overwrite the config file downloaded for another child, even if they are of the same type
