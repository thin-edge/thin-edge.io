
# thin-edge Data Model

The **data model** identifies all data send or received from/to **thin-edge** and its components, to interact with those.
For all data it defines format and explains behaviour.

## Telemetry Data

**Telemetry Data** consists of **measurements**, **events** and **alarms**. Each is defined by a set of data-elements, each with specific behaviour.

### Measurements
**measurements** carry values from physical **Sensors**[^1] or a device's **Domain Application**[^1];
e.g. voltage and current of an electricity meter, or current state of the manufacturing control process


A measurement can carry a **single value**, or **multiple values** all taken at a single point in time.
```javascript
   {
     // example for a single-value measurement
     "temperature":               // 'type_name' of that measurement
                    25.3,         // 'value' of that measurement
     "time": "2020-10-15T05:30:47+00:00",  // optional 'timestamp' of that measurement
   }
```
```javascript
   {
     // example for a multi-value measurement
     "current": {                // 'type_name' of that measurement
        "L1": 9.5,               // the 1st 'value' of that measurement, named as "L1"
        "L2": 1.3                // the 2nd 'value' of that measurement, named as "L2"
        // ...even more values can occur
    },
     "time": "2020-10-15T05:30:47+00:00",  // optional 'timestamp' of that measurement
   }
```


|Reference  |Description|
| --------- | --------- |
|`type_name`  |a string that identifies the measurement uniquely in context of the device|
|`value`      |the value that was sampled; can be named (especially in context of a multi-value measurement) or unnamed; must be an integer or floating point number|
|`timestamp`  |optional time that indicates when values were sampled; when not provided, thin-edge.io uses the current system time as the time of the sample; when provided must be conform to ISO 8601|

  * **behaviour of a measurement:**
    - thin-edge does not store any historical sampled values for measurements
    - there is no initialization value for measurements; i.e. a measurement is not visible on thin-edge before the 1st sample was sent to thin-edge

### Events
**events** are notifications that something happened on a device's environment or software system;
e.g. a sensor[^1] detected something like a door has been closed,
or a system notification that e.g. a user has started an ssh session

```javascript
{
    // example of an event
    "text": "A user just logged in",     // 'text' message of that event
    "time": "2021-01-01T05:30:45+00:00"  // optional 'timestamp' of that event
}
```

|Reference  |Description|
| --------- | --------- |
|`type_name`  |a string that identifies the event uniquely in context of the device; it is not part of the event's data elements, instead it is part of the MQTT topics when the event message is sent to thin-edge.io|
|`text`       |carries a human readable event-text; must be UTF-8 encoded|
|`timestamp`  |optional time that indicates when the event has occurred; when not provided, thin-edge.io uses the current system time as the time of the event; when provided must be conform to ISO 8601|

TODO: add somehow "an event can optionally contain additional custom information"

  * **behaviour of an event:**
    - thin-edge does not store any historical occurrences for events

### Alarms
**alarms** are notifications about some critical behaviour of the device's environment or software system;
e.g. when a temperature sensor detects a temperature went out of its valid range

```javascript
{
    // example for an alarm
    "text": "Temperature is very high",  // 'text' message of that alarm
    "time": "2021-01-01T05:30:45+00:00"  // optional 'timestamp' of that alarm
}
```

|Reference  |Description|
| --------- | --------- |
|`type_name`  |a string that identifies the alarm uniquely in context of the device; it is not part of the alarm's data elements, instead it is part of the MQTT topics when the alarm message is sent to thin-edge.io|
|`severity`   |a string that indicates the severity of the alarm; must be `critical`, `major`, `minor` or `warning`; it is not part of the alarm's data elements, instead it is part of the MQTT topics when the alarm message is sent to thin-edge.io |
|`text`       |carries a human readable alarm-text; must be UTF-8 encoded|
|`timestamp`  |optional time that indicates when the alarm has occurred; when not provided, thin-edge.io uses the current system time as the time of the alarm; when provided must be conform to ISO 8601|

TODO: add somehow "an alarm can optionally contain additional custom information"

  * **behaviour of an alarm:**
    - thin-edge does not store any historical occurrences for alarms
    - **alarms** are stateful; i.e. once raised, an **alarm** is active until it was explicitly cleared by the device's software or the cloud

[^1]: details see "Domain Model" appendix [Device Domain](./domain-model.md#device-overview) -->

## Use of MQTT

**thin-edge** expects the MQTT broker [mosquitto](https://mosquitto.org/) to be available on the device.
**thin-edge** uses **mosquitto** to consume and provide telemetry data. All telemetry data are reflected with specific MQTT topics and payload in JSON format.

**thin-edge** assumes **mosquitto** is configured in a secure manner, to avoid any inappropriate access to **thin-edge** topics and payload.
Any malicious access to the broker can hazard **thin-edge** and all connected devices. Mosquitto provides a wide range of authentication and access control options. For more details see _Authentication_ and _ACL_ (Access Control List) in [mosquitto documentation](https://mosquitto.org/man/mosquitto-conf-5.html).

### Telemetry Data

All telemetry data (**Measurements**, **Events**, **Alarms**) are reflected with MQTT topics, where each has it's specific subtopic (e.g. `tedge/measurements` or `tedge/events`).

  * each provider of a **measurement**, **event** or **alarm** sends the occurring data to **thin-edge's** MQTT broker
    * a provider can be the domain application[^1], other SW components / 3rd parties
  * all processes (e.g. the domain application[^1], other SW components / 3rd parties) on the main-device and all child-devices can consume those telemetry data from the MQTT broker
  * the cloud mapper on the **main-device** picks-up _all_ telemetry data from the MQTT broker and transfers those to the cloud

The communication diagram below illustrates that behaviour.

![thin-edge Inventory](images/MQTT-communication.svg)



#### Measurements

  * **topic**: `tedge/measurements`
  * **payload**: one or more measurements in JSON format, as below:
```javascript
   {
     // example for an MQTT message that contains two measurements

     // first measurement, e.g. single-value measurement
     "temperature": 25.3,

     // second measurement, e.g. multi-value measurement
     "current": {
        "L1": 9.5,
        "L2": 1.3
    },

    // ...even more measurements can be contained

     "time": "2020-10-15T05:30:47+00:00",  // optional timestamp for all measurements
   }
```
  * **MQTT retain flag**: A measurement should never be published as retain message.
                      That is as a single retained measurement might be consumed
                      and processed more than once by a consuming software
                      component (e.g. when that software component restarts and
                      subscribes again).

#### Events
  * **topic**: `tedge/events/<type_name>`
  * **payload**: the event in JSON format, as below:
```javascript
{
    // example for an event's MQTT message
    "text": "A user just logged in",
    "time": "2021-01-01T05:30:45+00:00"
}
```
  * **MQTT retain flag**: An event should never be published as retain message.
                      That is as a single retained event might be consumed
                      and processed more than once by a consuming software
                      component (e.g. when that software component restarts
                      and subscribes again).


#### Alarms
  * **topic**: `tedge/alarms/<severity>/<type_name>`
  * **payload**: the alarm in JSON format, as below:
```javascript
{
    // example for an alarm's MQTT message
    "text": "Temperature is very high",
    "time": "2021-01-01T05:30:45+00:00"
}
```
  * **MQTT retain flag**: All alarms shall be published as retain message to
                      reflect the alarm's stateful behaviour in the broker.
                      The retain messages is kept in the MQTT broker as long
                      as the alarm is raised.
                      When a raised alarm is gone again, an empty retain message
                      shall be published to clear the alarm message in the broker.

### Telemetry Data for Child-Devices

All telemetry data provided to the MQTT bus are associated by **thin-edge** and all consumers with the thin-edge **main-device** or some **child-device** (see more details about **child-devices** in the [domain model](./domain-model.md#child-devices)).

Therefore the `child-id` of the **child-device** is can be appended to the MQTT topic, if the message is meant for a **child-device**;
or no `child-id` is appended, if the message is meant for the **main-device**.

MQTT topics for the **main-device**:
```
tedge/measurements
tedge/events/<event-type>
tedge/alarms/<severity>/<alarm-type>
```

MQTT topics for a **child-device**, including the **child-device's** specific `child-id`:
```
tedge/measurements/<child-id>
tedge/events/<event-type>/<child-id>
tedge/alarms/<severity>/<alarm-type>/<child-id>
```


