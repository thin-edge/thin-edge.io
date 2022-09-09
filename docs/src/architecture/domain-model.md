# thin-edge Domain Model

The following section describes the relevant aspects and entities of the **thin-edge** domain.

**thin-edge** is designed to facilitate IoT functionality to resource constrained **devices**.
The focus is on industrial OT Devices or any other kind of Embedded Devices.
As the figure below illustrates those **devices** can by **PLCs**, or any kind of **SoC-based** or **Microcontroller-based** Embedded Systems.

![Device Class](images/device-class.svg)

* **PLC** (**P**rogrammable **L**ogic **C**ontroller):
  A PLC is specialized hardware that has been designed to control manufacturing processes.
  Its hardware has been ruggedized to operate in harsh electrical, thermic and mechanical industrial environments. 
  * The **Control Application** is a program, usually written by an **Automation Engineer** in a domain specific graphical programming language (e.g. "Ladder Diagram" or "Function block diagram"). 
    * It is developed with a specific Development Software on a PC (e.g. __STEP 7__ or __CODESYS__), and downloaded to the PLC.
  * The **RTS** (**R**un **T**ime **S**ystem) is the basic software components on a PLC, that
    * receives and accepts the **Control Application** from the Development Software on a PC
    * manages and observes the cyclic execution of the **Control Application**
  * **Sensors** and **Actuators** enable the PLC to observe and control physical behaviours on the shopfloor.
    * can built-in in the PLC device, 
      or connected to the PLC via some **Fieldbus** (e.g. Modbus, CANopen, PROFINET, EtherCAT, ...)
    * can be be simple inputs/outputs (as digital signals from a light barrier, or analouge values like temperature or pressure);
      or complex equipments as robotic arms or other PLCs
  * The **Process Image** is a block in the memory, that reflects all input and output values of all connected **Sensors** and **Actuators**
    * it is managed by the RTS during the cyclic execution of the **Control Application**, as steps below:
      * Step 1: RTS reads all inputs from Sensor's and Actuator's, and stores all read input values to the **Process Image's** _input area_ 
      * Step 2: RTS executes once the control application, that operates on the **Process Image's** _input_ and _output area_
      * Step 3: RTS reads all output values from the **Process Image's** _output area_, and writes all read values to Sensor's and Actuator's
      * then, next cycle starts with Step 1

* **SoC**-based (**S**ystem **o**n **C**hip) Embedded System:
  * OS:
    usually a proprietary real-time OS (e.g. QNX) or (Embedded) Linux
  * TODO

* **Microcontroller**-based Embedded System:
  * TODO

## thin-edge data representation

**thin-edge** provides different devices in a standardized representation to the cloud. Therefore **thin-edge** provides different kinds of data representation:
  * **Measurements**: contain numerical data produced by sensors (like temperature readings) or calculated data based on information from devices (service availability of a device)
  * **Events**: contain other real-time information from the sensor network, such as the triggering of a door sensor
  * **Alarms**: similar to Event, but the user or operator of the system has to take action to resolve the alarm
  * **Operations**: relate to data that is sent to devices for execution or processing, such as switching a relay in a power meter or sending a credit to a vending machine. TODO: using operations also to modify set-points or similar?
 
TODO: add a mapping of thin-edge data representation to PLC entities, etc. E.g.:
  * Measurements <-> output values from a **PLC's** **Process Image**
  * Events
  * Alarms
  * Operations
  
## Main Challenges

* Industrial automation area is very heterogen.
  * Even there are standards like IEC 61131-3 (that standardizes programming lanuages and behaviour) and common Fieldbus protocols, interoperability to industrial controllers and the diversity of peripheral devices from different manufucaters is hard to manage.
* Ressources constraint device hardware
* _Knowledge-Gap_ between OT and IT. Industrial application engineers (the PLC guys) are most often no software developers but experts for the manufacturing process. Instead software developers (e.g. the cloud guys) have less awareness for needs of industrial application engineers and the shopfloor.
* Many PLC sites have restrictected network access (especially in a manufaturing hall).


No yet covered:
* "capabilities", "fragments", "child-devices"
* details about signals or setpoints in the Process Image
* where is thin-edge deployed?
