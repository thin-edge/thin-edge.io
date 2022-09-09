# thin-edge Domain Model

The following section describes the relevant aspects and entities of the **thin-edge** domain.

**thin-edge** is designed to facilitate IoT functionality to resource constrained **devices**.
The focus is on industrial OT **devices** or any other kind of embedded **device**.
It is not reduced to **devices** that are capable to install and run thin-edge, but includes also **devices** that need another _(gateway) device_ aside, that executes thin-edge.

As the figure below illustrates, those **devices** can by **PLCs**, or any kind of **SoC-based** or **Microcontroller-based** Embedded Systems.


![Device Class](images/device-class.svg)

* **PLC** (**P**rogrammable **L**ogic **C**ontroller):
  A PLC is specialized hardware that has been designed to control manufacturing processes.
  Its hardware has been ruggedized to operate in harsh electrical, thermic and mechanical industrial environments. 
  * The **Control Application** is a program, usually written by an **Automation Engineer** in a domain specific graphical programming language (e.g. "Ladder Diagram" or "Function block diagram"). 
    * It is developed with a specific Development Software on a PC (e.g. __STEP 7__ or __CODESYS__), and downloaded to the PLC.
  * The **RTS** (**R**un **T**ime **S**ystem) is the basic software component on a PLC, that
    * receives and accepts the **Control Application** from the Development Software on a PC
    * manages and observes the cyclic execution of the **Control Application**
  * **Sensors** and **Actuators** enable the PLC to observe and control physical behaviours on the shopfloor.
    * can built-in in the PLC device, 
      or connected to the PLC via some **Fieldbus** (e.g. Modbus, CANopen, PROFINET, EtherCAT, ...)
    * can be simple inputs/outputs (as digital signals from a light barrier, or analouge values like temperature or pressure);
      or complex equipments as robotic arms or other PLCs
  * The **Process Image** is a block in the memory, that reflects inputs and outputs of all connected **Sensors** and **Actuators**
    * it contains an _input area_ and an _output area_, where both areas are arrays of **data points**
    * each **data point** carries a value of an input or output
    * it is managed by the RTS during the cyclic execution of the **Control Application**, as steps below:
      * Step 1: RTS reads all inputs from Sensor's and Actuator's, and stores all read input values to the **Process Image's** _input area_ 
      * Step 2: RTS executes once the control application, that operates on the **Process Image's** _input_ and _output area_
      * Step 3: RTS reads all output values from the **Process Image's** _output area_, and writes all read values to Sensor's and Actuator's
      * then, next cycle starts with Step 1

* **SoC**-based (**S**ystem **o**n **C**hip) Embedded System:
  * TODO

* **Microcontroller**-based Embedded System:
  * TODO

## thin-edge data concepts

**thin-edge** provides different devices in a standardized representation to the cloud. Therefore **thin-edge** provides different kinds of data concepts:
  * **Measurements**:
    * contain numeric data produced by sensors (like temperature readings) or calculated data based on information from the **control application**.
    * a **measurements** consists of one or more numeric **data points** and optionally meta information as names or units
  * **Variables**:
    * contain numeric data provided and used by the device, and can be sent to the device (as e.g. "set points")
    * a **variable** consists of one or more numeric **data points** and optionally meta information as names or units
  * **Events**:
    * contain other real-time information from the sensor network, such as the triggering of a door sensor
    * an **event** is triggered by a **data point** (e.g. when it changes to a specific value as `0` or `1`), and contains an optional message text.
  * **Alarms**:
    * similar to Events, but the user or operator of the system has to take action to resolve the alarm
    * an **alarm** is triggered by a **data point** (e.g. when it changes to a specific value as `0` or `1`), and contains an optional alarm text.
  * **Operations**:
    * relate to data that is sent to the devices for execution or processing,
      such as switching a relay in a power meter,
      or requesting the device to provide or update ressources (e.g. configuration files or software modules).

## thin-edge device concepts

**thin-edge** facilitates IoT functionality to the device it is running on, as well as to devices that are connected to that device.
Therefore **thin-edge** provides the **device concept** as below:
  * all devices are categorized as below:
    * the **main-device** is the device thin-edge is running on
    * **external child-devices** are the devices connected to the **main-device**
  * each device has a unique **device-id**
  * each device has a set of **capabilities**
    * a **capability** reflects a **measurement**, **variable**, **event**, **alarm** or **operation** the device supports

## Main Challenges

* Industrial automation area is very heterogen.
  * Even there are standards like IEC 61131-3 (that standardizes programming lanuages and behaviour) and common Fieldbus protocols, interoperability to industrial controllers and the diversity of peripheral devices from different manufucaters is hard to manage.
* Ressources constraint device hardware
* _Knowledge-Gap_ between OT and IT. Industrial application engineers (the PLC guys) are most often no software developers but experts for the manufacturing process. Instead software developers (e.g. the cloud guys) have less awareness for needs of industrial application engineers and the shopfloor.
* Many PLC sites have restrictected network access (especially in a manufaturing hall).


---------------------------------------------
No yet covered:
* "fragments"
* to be align with vision.md


