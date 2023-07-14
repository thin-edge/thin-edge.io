---
title: Thin Edge JSON
tags: [Concept, MQTT]
sidebar_position: 4
---

# Thin Edge JSON format

Thin Edge JSON is a lightweight format used in `thin-edge.io` to represent telemetry data:
measurements, events and alarms as well as operations: software update, configuration update etc.
These data are exchanged over the [MQTT bus](mqtt-bus.md) by the devices and services.

## Sending Telemetry Data

The `thin-edge.io` framework exposes some MQTT endpoints that can be used by local processes
and other devices connected over the network
to exchange data between themselves as well as to get some data forwarded to the cloud.

### Sending Telemetry Data to thin-edge.io

All topics with the prefix `tedge/` are reserved by `thin-edge.io` for this purpose.
To send measurements to `thin-edge.io`, the measurements represented in Thin Edge JSON format can be published
to the `tedge/measurements` topic.
Other processes running on the thin-edge device can subscribe to this topic to process these measurements.

If the messages published to this `tedge/measurements` topic is not a well-formed Thin Edge JSON,
then these messages won’t be processed by `thin-edge.io`, not even partially,
and an appropriate error message on why the validation failed will be published to a dedicated `tedge/errors` topic.
The messages published to this topic will be highly verbose error messages and can be used for any debugging during development.
You should not rely on the structure of these error messages to automate any actions as they are purely textual data
and bound to change from time-to-time.

Here is the complete list of topics reserved by `thin-edge.io` for its internal working:

| Topic                                                | Description                                                            |
|------------------------------------------------------|------------------------------------------------------------------------|
| `tedge/`                                             | Reserved root topic of `thin-edge.io`                                  |
| `tedge/measurements`                                 | Measurements related to the main device                                |
| `tedge/measurements/${child-id}`                     | Measurements related to the child device named `${child-id}`           |
| `tedge/events/${event-type}`                         | Events related to the main device                                      |
| `tedge/events/${event-type/${child-id}`              | Events related to the child device named `${child-id}`                 |
| `tedge/alarms/${severity}/${alarm-type}`             | Alarms related to the main device                                      |
| `tedge/alarms/${severity}/${alarm-type}/${child-id}` | Alarms related to the child device named `${child-id}`                 |
| `tedge/errors`                                       | Error messages emitted by `thin-edge.io` while processing measurements |

### Sending Telemetry Data to the cloud

The `thin-edge.io` framework allows users forward telemetry data generated and published to one of the
`tedge/#` MQTT topics from the thin-edge device to any IoT cloud provider that it is connected to,
with the help of a *mapper* component designed for that cloud.
The responsibility of a mapper is to subscribe to the `tedge/#` topic to receive all incoming data
represented in the cloud vendor neutral Thin Edge JSON format, to a format that the connected cloud understands.
Refer to [Cloud Message Mapper Architecture](./tedge-mapper.md) for more details on the mapper component.

## Measurements

*Measurements* carry values from physical sensors, the domain application or monitored processes.
For instance:
- voltage and current of an electricity meter
- state of the manufacturing control process
- free disk space on the device

Thin Edge JSON can be used to represent single-valued measurements, multi-valued measurements
or a combination of both along with some auxiliary data like the timestamp at which the measurement(s) was generated.

### Single-valued measurements

Simple single-valued measurements like temperature or pressure can be expressed as follows:

```sh te2mqtt
tedge mqtt pub tedge/measurements '{
  "temperature": 25
}'
```

The key represents the measurement type, and the value represents the measurement value.
The keys can only have alphanumeric characters, and the underscore (`_`) character but must not start with an underscore.
The values can only be numeric.
String, Boolean or other JSON object values are not allowed.

### Multi-valued measurements

Like the name suggests, a multi-valued measurement is allowed to contain more than one value.
Here is the representation of a `three_phase_current` measurement that consists of `L1`, `L2` and `L3` values,
representing the current on each phase:

```sh te2mqtt
tedge mqtt pub tedge/measurements '{
  "three_phase_current": {
    "L1": 9.5,
    "L2": 10.3,
    "L3": 8.8
  }
}'
```

The key is the top-level measurement type and value is a JSON object having further key-value pairs 
representing each aspect of the multi-valued measurement.
Only one level of nesting is allowed, meaning the values of the measurement keys at the inner level can only be numeric values.

**❌ Example: Invalid measurement due to nesting > 2 levels**

```sh te2mqtt
tedge mqtt pub tedge/measurements '{
  "three_phase_current": {
    "phase1": {
      "L1": 9.5
    },
    "phase2": {
      "L2": 10.3
    },
    "phase3": {
      "L3": 8.8
    }
  }
}'
```

### Grouping measurements

Multiple single-valued and multi-valued measurements can be grouped into a single Thin Edge JSON message as follows:

```sh te2mqtt
tedge mqtt pub tedge/measurements '{
  "temperature": 25,
  "three_phase_current": {
    "L1": 9.5,
    "L2": 10.3,
    "L3": 8.8
  },
  "pressure": 98
}'
```

The grouping of measurements is usually done to represent measurements collected at the same instant of time.

### Auxiliary measurement data

When `thin-edge.io` receives a measurement, it will add a timestamp to it before any further processing.
If the user doesn't want to rely on `thin-edge.io` generated timestamps,
an explicit timestamp can be provided in the measurement message itself by adding the time value as a string 
in ISO 8601 format using `time` as the key name, as follows:

```sh te2mqtt
tedge mqtt pub tedge/measurements '{
  "time": "2020-10-15T05:30:47+00:00",
  "temperature": 25,
  "location": {
    "latitude": 32.54,
    "longitude": -117.67,
    "altitude": 98.6
  },
  "pressure": 98
}'
```

The `time` key is a reserved keyword and hence can not be used as a measurement key.
The `time` field must be defined at the root level of the measurement JSON and not allowed at any other level,
like inside the object value of a multi-valued measurement.
Non-numeric values like the ISO 8601 timestamp string are allowed only for such reserved keys and not for regular measurements. 

Here is the complete list of reserved keys that has special meanings inside the `thin-edge.io` framework
and hence must not be used as measurement keys:

| Key | Description |
| --- | --- |
| time | Timestamp in ISO 8601 string format |
| type | Internal to `thin-edge.io` |


## Events

*Events* are notifications that something happened on the device, its environment, the domain application or the software system.
For instance:
- a door has been closed
- a process started
- a user has started an ssh session

```sh te2mqtt
tedge mqtt pub tedge/events/login '{
  "text": "A user just logged in",
  "time": "2021-01-01T05:30:45+00:00",
  "someOtherCustomFragment": {
    "nested": {
      "value": "extra info"
    }
  }
}'
```

| Reference    | Description                                                                                                                                                        |
|--------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `event_type` | Uniquely identifies the event in the context of the device; part of the MQTT topic                                                                                 |
| `text`       | Text description of  the event; must be UTF-8 encoded                                                                                                              |
| `timestamp`  | Optional time that indicates when the event occurred, in ISO 8601 string format; when not provided, thin-edge.io uses the current system time                      |
| `*`          | Additional fields are handled as custom specific information; if the connected cloud supports custom fragments its mapper transfers those accordingly to the cloud |

## Alarms

*Alarms* are notifications about some critical behaviour of the device's environment or software system.
For instance:
- a temperature going out of its valid range
- a process that crashed
- free disk space going critically low

```sh te2mqtt
tedge mqtt pub tedge/alarms/warning/temperature_high '{
  "text": "Temperature is very high",
  "time": "2021-01-01T05:30:45+00:00",
  "someOtherCustomFragment": {
    "nested": {
      "value": "extra info"
    }
  }
}'
```

| Reference    | Description                                                                                                                                                        |
|--------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `alarm_type` | Uniquely identifies the alarm in the context of the device; part of the MQTT topic                                                                                 |
| `severity`   | Severity of the alarm; must be `critical`, `major`, `minor` or `warning`; part of the MQTT topic                                                                   |
| `text`       | Text description of the alarm; must be UTF-8 encoded                                                                                                               |
| `timestamp`  | Optional time that indicates when the alarm has occurred, in ISO 8601 string format; when not provided, thin-edge.io uses the current system time                  |
| `*`          | Additional fields are handled as custom specific information; if the connected cloud supports custom fragments its mapper transfers those accordingly to the cloud |

