# thin-edge Domain Model (no device)

The following section introduces the **thin-edge** domain model.

**thin-edge** is the glue between the **cloud** and **devices**. I.e. it interfaces on the one hand **devices** (especially resource constrained industrial **devices**), and connects on the other hand the **cloud**.
The **cloud** could be some IoT Cloud Platform, as e.g. [Cumulocity](https://www.softwareag.cloud/site/product/cumulocity-iot.html), [Azure](https://azure.microsoft.com) or [AWS](https://aws.amazon.com); or an on-premise "IoT system".

**thin-edge** has been designed with the assumption that the **device** is represented in the **IoT cloud** as **device twin**.
The **device twin** in the IoT cloud is a logical representation of the **device**. As such, it stores the _state_ of the **device**.
Here _state_ refers to e.g. **Telemetry Data** or **Device Management** details as _installed software versions_ or _configurations_.

### Telemetry Data

**Telemetry data** are the measurements, events and alarms collected or raised by sensors, actuators or domain applications.

  * **Measurement**, is a single value or set of values
    * could be be a mix of values produced by one or more sensors and values calculated by the device's domain application
    * values could be a mix of numbers, strings or booleans
    * has one timestamp
    * releates to one **Metric**
  * **Metric**, is a time-series of measurements
    * contains a _source device_
    * contains a _type name_
    * optionally contains _units_ for the **measurements**
  * TODO: Example for metric and it's measurements
  * **Command**, is a single value or set of values
    * is send from the cloud to one device, e.g. to
      - stimulate an actuator (e.g. switching a relay)
      - send a signal to the domain application
      - set one or more set-points (e.g. upper/lower limits or threshold of a climate control)
    * values could be a mix of numbers, strings or booleans
  * **Event**, is a notification that something happened on the device's environment or software system
    * it's source could be
      - a sensor that detected a door has beed closed
      - a signal from the device's domain application
      - a device's system notification that a user has started an ssh session
    * has one timestamp given by the producer, or implicitly set as the current time of receiving at **thin-edge**
  * **Alarm**, is similar to an event, but the _End User_ (an operator of the system) has to take action to resolve the alarm.

### Device Management

**Device Management** provides to manage and monitor devices in the field from the cloud. That includes:
  * **Software Management**, provides to manage the installed software packages on the device
    * retrieve list and versions of installed packages
    * install new or update existing software packages
    * remove installed software packages
  * **Configuration Management**, provides to view and change configurations on the device
    * lists available configuration files
    * transfer individual configuration files from device to cloud, and vice versa
  * **Log Management**, provides to view log files from the device
    * lists available log files
    * transfer individual log files from device to cloud
  * **Device Monitoring**, collects metrics from the device and forward these to the IoT cloud
    * allows monitors the health of devices
    * helps to troubleshoot when problems with the device are reported

## thin-edge device concept

**thin-edge** facilitates IoT functionality to the device it is running on, as well as to devices that are connected to that device.

  * the device thin-edge is running on is referred as the **main-device**
    * thin-edge on the **main-device** establishes and manages all communication to the cloud
  * all devices connected to the **main-device** are referred as **external child-devices**
  * each device is represented in the cloud with an individual **device twin**
  * a unique **child-id** makes the association between each **external child-device** and it's **device twin**
  * all telemetry data (i.E. Measurements, Commands, Events, Alarms) of a device (i.E. **main-device** or **external child-devices**) appear
    in the cloud in context of its corresponding **device twin**

The figure below illustrates the device concept.

![Device Concept](images/device-concept.svg)

## thin-edge data concept

**thin-edge** provides APIs to easily connect device's **data points** to cloud's **telemetry data handling**.
**thin-edge** never accesses device's **data points** directly.
In any case there is one or more _custom specific sw component_ that interface **data points** and **thin-edge** APIs (e.g. the **domain application** and some kinds of **protocol drivers**).

Those _custom specific sw components_ provide **data point** values to the **thin-edge** APIs as **measurements**, **commands**, **events** or **alarms**.

  * **Measurements**:
    * a **measurement** is represented with one or more **data point** values
      and a reference to the corresponding **metric**
    * the values of the **data points** reflect the measurement values
    * **thin-edge** puts the measurement into the context of the corresponding metric and sends it to the cloud
  * **Command**:
    * a **command** is represented with one or more values received from the **cloud**
    * **thin-edge** provides those values to the **domain application** and **protocol drivers** as **data point** values
  * **Events**:
    * an event is represented by a **data point**
    * the meaning of an event related **data point** value is very customer specific (could be change from 0 to 1, a bit in a flag word, ...)
    * a _custom specific sw component_ must know the meaning, and sends an event notification to the **thin-edge** API whenever the value signals a raised event
  * **Alarms**:
    * similar to **events**; but in addition, the _custom specific sw component_ can send a notification to **thin-edge** to clear an **alarm** 
    * **thin-edge** raises or clears the alarm in the cloud

## thin-edge device management concept

**thin-edge** maps cloud's different **Device Management** functionalities to different resources of the device:
  * **Software Management**:
    * software packages are installable units on the device
    * those units could be
      * the **domain application**
      * parts from **OS / Libs / Runtime**, or the whole thing as one
    * examples for software packages are
      * packages for a Linux Packages Managers (e.g. for Debian, ...)
      * container images (e.g. for Docker)
      * simple ZIP files
      * custom specific files/packages
  * **Configuration Management**:
    * a configuration is a text file or a binary file
    * those configurations could be
      * configuration file(s) of the **domain application**
      * one or more configurations file of **OS / Libs / Runtime**
  * **Log Management**:
    * a Log is a log file, could be
      * log file(s) of the **domain application**
      * one or more log files of **OS / Libs / Runtime**

**thin-edge** realizes cloud's **Device Management** based on **plugins**.
  * a **plugin** encapsulates and manages access to _ressources_ and _services_ of the device, as e.g.
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
  * a **child-device agent** can also be used on the **main-device**, without appearing in the cloud as child-device, e.g. in order to
    * provide container resources (e.g. config files) to a **plugin** running in another container; by running the **child-device agent** inside the resource's container
    * allow to access _resources_ of the **main-device** somehow differently as the plugin's implementation does
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

TODO: consider containers here?


# Appendix

## Device Domain

TODO: **thin-edge** is designed to facilitate IoT functionality to resource constrained **devices**. 
      ... that section is to ...

The focus is on industrial OT **devices** or any other kind of embedded **devices**. It is not reduced to **devices** that are capable to install and run thin-edge, but includes also **devices** that need another _(gateway) device_ aside, that executes **thin-edge**.

Usual **devices** are **PLCs** (**P**rogrammable **L**ogic **C**ontrollers), **IPCs** (**I**ndustrial **PC**s) or any kind of **SoC-based** or **Microcontroller-based** Embedded System. The figure below shows a simplified conceptual model of such a device.

![Simple Device Model](images/simple-device-model.svg)

<!--
* TODO: add somehow "Such a **device** is most often a specialized hardware that has been ruggedized to operate in harsh electrical, thermic and mechanical industrial environments."
-->
* The **Domain Application** is a program, that contains the domain specific process logic.
  * On a **PLC** it's a _control application_, created by an _automation engineer_ in a domain specific graphical programming language (like "Ladder Diagram" or "Function block diagram")
  * Or on a **SoC-based** or **Microcontroller-based** system it's an _application program_, created by an _embedded software engineer_ usually in C/C++
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



---------------------------------------------
Open Topics:
* "fragments"
* better word for plugin
* better word for child-device agent (maybe child-device proxy)
