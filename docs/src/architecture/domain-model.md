# thin-edge Domain Model

The following section introduces the **thin-edge** domain model.

The **thin-edge** domain model explains details of target domains **thin-edge** is primarily designed for.
It identifies _entities_ and _aspects_ of those target domains, that are touch points for **thin-edge**.
Additionally it introduces new _entities_ or _aspects_ to seamlessly fit **thin-edge** into those target domains.

Finally the **thin-edge** domain model gives..
  * _target domain experts_ a clear idea how to position **thin-edge** in their target domain
  * _target domain experts_ **and** _thin-edge developers_ a common understanding and unique vocabulary for **thin-edge** and it's environment

## thin-edge target domains

**thin-edge** is designed to facilitate IoT functionality to resource constrained **devices**.
The focus is on industrial OT **devices** or any other kind of embedded **devices**. It is not reduced to **devices** that are capable to install and run thin-edge, but includes also **devices** that need another _(gateway) device_ aside, that executes **thin-edge**.

Usual **devices** are **PLCs** (**P**rogrammable **L**ogic **C**ontrollers), **IPCs** (**I**ndustrial **PC**s) or any kind of **SoC-based** or **Microcontroller-based** Embedded System. The figure below shows a simplified conceptual model of such a device.

![Simple Device Model](images/simple-device-model.svg)

<!--
* TODO: add somehow "Such a **device** is most often a specialized hardware that has been ruggedized to operate in harsh electrical, thermic and mechanical industrial environments."
-->
* The **Domain Application** is a program, usually developed by a **Domain Expert**
  * e.g. on a **PLC** it's a _control application_, created by an _automation engineer_ in a domain specific graphical programming language (like "Ladder Diagram" or "Function block diagram")
  * or on a **SoC-based** or **Microcontroller-based** system it's an _application program_, created by an _embedded software engineer_ usually in C/C++
* The **OS / Libs / Runtime** provide basic functionality to the **Domain Application**
  <!-- TODO: add somehow "used to abtracts the hardware. But: on a microcontroler usually less abtraction, more hw dep on the domain app, and even no OS" -->
* **Sensors** and **Actuators** enable the **device** to observe and control physical behaviour on the shopfloor or device's environment.
  * can be integrated in the **device's** hardware,
    or connected to the **device** via some **Fieldbus** (e.g. Modbus, CANopen, PROFINET, EtherCAT, ...) or
    some **Local Interface** as USB, UART, SPI, I2C, ...
  * can be simple peripherals as a light barrier, or a sensor for temperature or pressure;
    or complex equipments as robotic arms or even other **devices**
* **Inputs / Outputs** are the communication channels between the **Domain Application** and **Sensors** and **Actuators**
  * drivers (as part of the **OS / Libs / Runtime** and/or the **Domain Application**) do expose all data from
    **Sensors** and **Actuators** to the **device** as inputs or outputs
  * also the **Domain Application** can expose data as input or output (e.g. own _signals_ or _states_)

## thin-edge data concepts

**thin-edge** provides different devices in a standardized representation to the cloud. Therefore **thin-edge** provides specialized data concepts.

The data concepts below belong to **inputs** and **outputs** available on a device. Thereby each **input** and **output** is refered as a **data point**.

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

## thin-edge device concept

**thin-edge** facilitates IoT functionality to the device it is running on, as well as to devices that are connected to that device.
All devices are categorized as below:
  * the **main-device** is the device thin-edge is running on
  * all devices connected to the **main-device** are referred as **external child-devices**
  * each device has a unique **device-id**

The figure below illustrates the device concept.

![Device Concept](images/device-concept.svg)

## thin-edge plugin concept

**thin-edge** realizes full-fledged cloud functionality (e.g. _configuration management_ or _log management_) with **plugins**.
  * a **plugin** encapsulates and manages accesses to _ressources_ and _services_ of the device, as e.g.
      * _software management_ accesses the device's _package manager_
      * _configuration management_ accesses device's _configuration files_
  * a **plugin** can be
      * an (external) executable (e.g. as the `c8y_configuration_plugin` for _configuration management_)
      * or a thin-edge built-in software component (e.g. as for _software management_)
  * usually a **plugin** runs on the **main-device**; thus it can access the _resources_ of the **main-device** directly
  * to access _ressources_ of an **external child-device** a **plugin** needs another component, referred as **child-device agent**

The figure below illustrates the concept of **plugins** and **child-devices agents**.

![Plugin Concept](images/plugin-concept.svg)

### Child-Device Agent
  * a **child-device agent** is the counterpart of a **plugin**, that takes the responsibility to access to the **external child-device's** _resources_
  * a **child-device agent** can serve one or more **plugins**
  * a **child-device agent** can be installed and executed on the **external child-device**, or on the **main-device**
    * if it runs in the **external child-device** it can access the _resources_ directly
    * if it runs on the **main-device** it can use any (low-level) interfaces the **external child-device** provides to access those resources
      * One main reason to install the **child-device agent** on the **main-device** is, when the **external child-device** cannot or shall not be altered.

### Plugin-Contract

A **plugin** defines and implements a specific **contract** for all interactions with a **child-device agent**
  * part of the **contract** could be e.g.:
      * the **child-device agent** must listen and react to certain requests of the **plugin**, e.g. on MQTT
      * the **child-device agent** must provide/consume files to/from the **plugin** on purpose, e.g. via HTTP
      * ...and more...
  * a **plugin's** **contract** can be denoted with a unique name (e.g. `tedge_config`)
    * based on that unique name a **child-device agent** can report and find **plugins** the child-device intends to contact (e.g. during providioning phase)
    * those information can be also provided to the cloud and other applications on the device site, on purpose

## Main Challenges

* Industrial automation area is very heterogen.
  * Even there are standards like IEC 61131-3 (that standardizes programming lanuages and behaviour) and common Fieldbus protocols, interoperability to industrial controllers and the diversity of peripheral devices from different manufucaters is hard to manage.
* Ressources constraint device hardware
* _Knowledge-Gap_ between OT and IT. Industrial application engineers (the PLC guys) are most often no software developers but experts for the manufacturing process. Instead software developers (e.g. the cloud guys) have less awareness for needs of industrial application engineers and the shopfloor.
* Many PLC sites have restrictected network access (especially in a manufaturing hall).


---------------------------------------------
Open Topics:
* "fragments"
* to be align with vision.md
* better word for plugin
* better word for child-device agent (maybe child-device proxy)
