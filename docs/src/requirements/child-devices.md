
# Child Device Concept

TODO: Add motivation for child-device concept. Somethin like:<br/>
-- Device-side and Cloud-side structural view (that might match the physical structure)<br/>
-- Features are available per child-device (e.g. Software Management with one SW list per child device, ...)

That document summarises a collection of use-cases and requirements, based on experiences of Sofware AG and some customers/partners.

## Definition
1) A child device is a physical device connected to the thin-edge device (typically a gateway device)  or even over IP.
2) Generally there are two kinds of child devices:
   - a) A dumb device that is connected to the thin-edge device via an IO-Link or fieldbus, that can't run the processing logic that sends/receives data to/from the thin-edge device. For such devices, the processing logic will be running on the thin-edge device itself.
   - b) A smart device that is connected to the thin-edge device over an IP network, with the processing logic running on that device itself. Its interactions with the thin-edge device will only be over the network
3) In both kinds of devices, there will always be a "child device process" that is responsible for handling all data to/from the physical child device.
4) Thin-edge's responsibility is to receive and route any cloud-bound data from these child device processes and any device-bound data like software/config binaries to these child device processes.
5) The child device process itself is resposible for pushing these binaries to the physical device itself and applying it there.
6) Each child device has a unique ID that can be used to identify/distinguish between them

TODO: Add somehow statement below as definition: 
"PLCs are already long time in production-line and no additional software can be installed (e.g. MQTT client) or configuration changed, to make the PLC capbable to communicate with the thin-edge device. That is to avoid any risc in the production line. Instead the thin-edge device must use existing interfaces from the PLC."

## Questions
1) Are child devices and their unique IDs static and known at thin-edge provisioning time, or do they dynamically get connected to thin-edge?
2) Should thin-edge provide routing of data between child devices (from one child device to another)?
3) Will there be one "child device process" per "physical child device"? Or a single process can manage multiple physical child devices at once?
4) Should thin-edge also support receiving data from multiple child devices and combining all that data and associate it with the main thin-edge device twin itself in the cloud(instead of having multiple child devices)? Customer typically do this as a cost-cutting measure with clouds charging fees per device.

## Use-Cases

* **Use-Case 1: Gathering telemetry data from connected PLCs and handle as Measurements, Alarms or Events**
   - a process on the thin-edge device collects process-data from a CODESYS PLC via local network (using the CODESYS PLC handler)
   - per each connected PLC a process is running
   - the process pushes gathered PLC's process-data to thin-edge measurements, alarms or events
   - the process has a configuration file that contains mapping information, meaning which process-data to push to which thin-edge measurement, alarm or event;
     and which child-device ID to use
   

* **Use-Case 2: Applying/updating data-mapping configuration of PLCs using Configuration Management**
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
