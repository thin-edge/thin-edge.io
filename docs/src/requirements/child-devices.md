
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

## Questions
1) Are child devices and their unique IDs static and known at thin-edge provisioning time, or do they dynamically get connected to thin-edge?
2) Should thin-edge provide routing of data between child devices (from one child device to another)?
3) Will there be one "child device process" per "physical child device"? Or a single process can manage multiple physical child devices at once?
4) Should thin-edge also support receiving data from multiple child devices and combining all that data and associate it with the main thin-edge device twin itself in the cloud(instead of having multiple child devices)? Customer typically do this as a cost-cutting measure with clouds charging fees per device.

## Use-Cases

* **Use-Case 1: Gathering telemetry data from connected PLCs and handle as Measurements, Alarms or Events**
   - a process on the thin-edge device collects process-data from a CODESYS PLC via local network (using the CODESYS PLC handler)
   - per each connected PLC a process is running
   - the process has a configuration file that contains mapping information, meaning which process-data to push to which thin-edge measurement, alarm or event;
     and which child-device ID to use
   

* **Use-Case 2: Applying/updating data-mapping configuration of PLCs using Configuration Management**
   - the process' configuration (see process configuration file in use-case 1) is managed by Configuration Management
   - each PLC appears as child-device in the Cloud, that supports individual Configuration Management functionality per child-device 

* **Use-Case 3: Update application on connected PLCs using Software Management**
   - a commandline tool on the thin-edge device is capable to remove or update the CODESYS application running on the connected PLCs (using the CODESYS PLC handler)
   - a Software Management plugin (one for all PLCs or individual plugins per PLC) uses the commandline tool remove or update 
     the CODESYS application on the according PLC on request
   - each PLC appears as child-device in the Cloud, that supports individual Software Management functionality per child-device 


NOTE 1: The PLCs are already long time in production and must not be touched/modified (to avoid any risc in the production line).
NOTE 2: Could be also another PLC than CODESYS or even another device than a PLC.
NOTE 3: Use-case(s) with external device than can directly access thin-edge interfaces (e.g. tedge/measurements) not yet covered.

# Requirements

- **Requirement 1:** Thin-edge has to provide child-device support for interfaces: <br/>
     - Measurements/Alarm/Events<br/>
     - Configuration Managements<br/>
     - Software Managements Plugin API<br/>

- **Requirement 2:** Where thin-edge provides child-device support it uses according meachnisms from the Cloud to appear according data/functionality as own device underneath the thin-edge device in the Cloud.

- **Requirement 3:** When data is pushed to thin-edge for a child-device, thin-edge will take care that the child-device exists in the Cloud, and creates it if needed.

- **Requirement 4:** Thin-edge is stateless in terms of child devices. I.e. it does not store a list of child-devices of even a state of those. All knowledge about child-devices is in the applications that provide/consume data and functionality from/to child-devices.  

