---
title: Thin Edge JSON
tags: [Concept, MQTT]
sidebar_position: 1
---

# Thin Edge JSON format

Thin Edge JSON is a lightweight format used in `thin-edge.io` to represent measurements data.
This format can be used to represent single-valued measurements, multi-valued measurements
or a combination of both along with some auxiliary data like the timestamp at which the measurement(s) was generated.

## Single-valued measurements

Simple single-valued measurements like temperature or pressure measurement with a single value can be expressed as follows:

```json
{
  "temperature": 25
}
```

where the key represents the measurement type, and the value represents the measurement value.
The keys can only have alphanumeric characters, and the "_" (underscore) character but must not start with an underscore.
The values can only be numeric.
String, Boolean or other JSON object values are not allowed.

## Multi-valued measurements

A multi-valued measurement is a measurement that is comprised of multiple values. Here is the representation of a
`three_phase_current` measurement that consists of `L1`, `L2` and `L3` values, representing the current on each phase:

```json
{
  "three_phase_current": {
    "L1": 9.5,
    "L2": 10.3,
    "L3": 8.8
  }
}
```

where the key is the top-level measurement type and value is a JSON object having further key-value pairs 
representing each aspect of the multi-valued measurement.
Only one level of nesting is allowed, meaning the values of the measurement keys at the inner level can only be numeric values.
For example, a multi-level measurement as follows is NOT valid: 

```json
{
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
}
```

because the values at the second level(`phase1`, `phase2` and `phase3`) are not numeric values.

## Grouping measurements

Multiple single-valued and multi-valued measurements can be grouped into a single Thin Edge JSON message as follows:

```json
{
  "temperature": 25,
  "three_phase_current": {
    "L1": 9.5,
    "L2": 10.3,
    "L3": 8.8
  },
  "pressure": 98
}
```

The grouping of measurements is usually done to represent measurements collected at the same instant of time.

## Auxiliary measurement data

When `thin-edge.io` receives a measurement, it will add a timestamp to it before any further processing.
If the user doesn't want to rely on `thin-edge.io` generated timestamps,
an explicit timestamp can be provided in the measurement message itself by adding the time value as a string 
in ISO 8601 format using `time` as the key name, as follows:

```json
{
  "time": "2020-10-15T05:30:47+00:00",
  "temperature": 25,
  "location": {
    "latitude": 32.54,
    "longitude": -117.67,
    "altitude": 98.6
  },
  "pressure": 98
}
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

## Sending measurements to thin-edge.io

The `thin-edge.io` framework exposes some MQTT endpoints that can be used by local processes
to exchange data between themselves as well as to get some data forwarded to the cloud.
It will essentially act like an MQTT broker against which you can write your application logic.
Other thin-edge processes can use this broker as an inter-process communication mechanism by publishing and 
subscribing to various MQTT topics.
Any data can be forwarded to the connected cloud-provider as well, by publishing the data to some standard topics.

All topics with the prefix `tedge/` are reserved by `thin-edge.io` for this purpose.
To send measurements to `thin-edge.io`, the measurements represented in Thin Edge JSON format can be published 
to the `tedge/measurements` topic.
Other processes running on the thin-edge device can subscribe to this topic to process these measurements.

If the messages published to this `tedge/measurements` topic is not a well-formed Thin Edge JSON, 
then that message won’t be processed by `thin-edge.io`, not even partially,
and an appropriate error message on why the validation failed will be published to a dedicated `tedge/errors` topic.
The messages published to this topic will be highly verbose error messages and can be used for any debugging during development.
You should not rely on the structure of these error messages to automate any actions as they are purely textual data 
and bound to change from time-to-time.

More topics will be added under the `tedge/` topic in future to support more data types like events, alarms etc.
So, it is advised to avoid any sub-topics under `tedge/` for any other data exchange between processes.

Here is the complete list of topics reserved by `thin-edge.io` for its internal working:

| Topic | Description |
| --- | --- |
| `tedge/` | Reserved root topic of `thin-edge.io` |
| `tedge/measurements` | Topic to publish measurements to `thin-edge.io` |
| `tedge/measurements/<child-id>` | Topic to publish measurements to `thin-edge.io`'s child device |
| `tedge/errors` | Topic to subscribe to receive any error messages emitted by `thin-edge.io` while processing measurements|

## Sending measurements to the cloud

The `thin-edge.io` framework allows users forward all the measurements generated and published to
`tedge/measurements` MQTT topic in the thin-edge device to any IoT cloud provider that it is connected to,
with the help of a *mapper* component designed for that cloud.
The responsibility of a mapper is to subscribe to the `tedge/measurements` topic to receive all incoming measurements 
represented in the cloud vendor neutral Thin Edge JSON format, to a format that the connected cloud understands.
Refer to [Cloud Message Mapper Architecture](./mapper.md) for more details on the mapper component.
