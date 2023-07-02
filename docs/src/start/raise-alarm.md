---
title: Raising Alarms
tags: [Getting Started, Telemetry]
sidebar_position: 7
---

# Raising Alarms

Alarms on thin-edge.io can be used to create alerts, represent state changes etc.
For example, an alarm can be raised when a certain measurement value breaches some threshold (like high temperature) or when an unexpected event occurs in the system (like a sensor failure).

A typical alarm cycle starts by the raising of an alarm by some monitoring process which alerts a system/human of an event needing some action.
Once some action is taken, the alarm is cleared explicitly by that system/human.

Every alarm is uniquely identified by its type and severity.
That is, for a given alarm type, alarms of varying severities are treated as independent alarms and hence, must be acted upon separately.
For an alarm of a given type and severity, only the last known state is considered relevant.
Thin-edge.io doesn't keep a history of all its state changes but only reacts to the last one it receives.


## Raising an alarm

An alarm can be raised on thin-edge.io by sending an MQTT message in Thin Edge JSON format to certain MQTT topics.

The scheme of the topic to publish the alarm data is as follows:

```text title="Topic"
tedge/alarms/<severity>/<alarm-type>
```

The payload format must be as follows:

```json title="Payload"
{
  "text": "<alarm text>",
  "time": "<Timestamp in ISO-8601 format>"
}
```

:::note
These messages must be sent with MQTT retained flag enabled and with QOS > 1 to ensure guaranteed processing by thin-edge.io.
Enabling the retained flag ensures that the alarm stays persisted with the MQTT broker until its state changes again.
These retained messages will make sure that the thin-edge.io processes or any other third-party processes subscribed to these alarms will get those,
even if they were down at the moment the alarm was raised.
If multiple messages are sent to the same alarm topic, the last alarm is considered to have overwritten the previous one.
:::

Here is a sample alarm raised for `temperature_high` alarm type with `critical` severity:

```text title="Topic"
tedge/alarms/critical/temperature_high
```

```json title="Payload"
{
  "text": "Temperature is very high",
  "time": "2021-01-01T05:30:45+00:00"
}
```

:::note
Both the `text` field and the `time` field are optional.
When a `text` is not provided, it is assumed to be empty.
When `time` is not provided, thin-edge.io will use the current system time as the `time` of the alarm.
When you want to skip both fields, use an empty json fragment `{}` as the payload to indicate the same.
An empty message can't be used for the same, as empty messages are used to clear alarms, which is discussed in the next section.
:::

The `<severity>` value in the MQTT topic can only be one of the following values:

1. critical
2. major
3. minor
4. warning

There are no such restrictions on the `<alarm-type>` value.

Thin-edge.io doesn't keep any history of all alarms raised on an alarm topic.

## Clearing alarms

An already raised alarm can be cleared by sending an empty message with retained flag enabled to the same alarm topic on which the original alarm was raised.

For example `temperature_alarm` will be cleared by publishing an empty payload message as below:

```sh te2mqtt
tedge mqtt pub tedge/alarms/critical/temperature_alarm "" -q 2 -r
```

:::note
Using the retained (-r) flag is a must while clearing the alarm as well, without which the alarm won't be cleared properly.
:::

If alarms of different severities exist for a given alarm type, they must all be cleared separately as they're all treated as independent alarms.

### Raising alarms from child devices

Alarms for child devices can be raised by publishing the alarm payload to `tedge/alarms/<severity>/<alarm-type>/<child-device-id>` topic,
where the `child-device-id` is the unique device id of the child device.
The alarm payload structure is the same, as described in the previous section.

### Raising an alarm with custom fragment

Thin-edge supports the creation of alarms using custom (user-defined) fragments.
Custom fragments are supported for both the main and child devices.
The custom fragments can be a simple json value or a complex json value.

For example, an alarm with simple custom fragment field named `details`:

```json title="Payload"
{
  "text": "Temperature is very high",
  "time": "2021-01-01T05:30:45+00:00",
  "details": "A custom alarm info"
}
```

For example, an alarm with complex custom fragments

```json title="Payload"
{
  "text": "Temperature is very high",
  "time": "2021-01-01T05:30:45+00:00",
  "someOtherCustomFragment": {
    "nested": {
      "value": "extra info"
    }
  }
}
```

:::note
Other than `text` and `time` fields, all the other fields are considered as custom fragments.
:::

### Raising an alarm with empty json payload

Alarms can also be raised with an empty json object as payload as follows:

```json title="Payload (empty json object)"
{}
```

:::note
The `default` value for the `time` fragment will be the timestamp in utc time that is added by the `tedge-mapper-c8y`
while alarm message being translated to cumulocity format.
The default value for the `text` fragment will be derived from the `alarm-type` of the topic.
:::

## Cloud data mapping

If the device is connected to some supported IoT cloud platform, any alarms raised locally on thin-edge.io will be forwarded to the connected cloud platform as well.
The mapping of thin-edge alarms data to its respective cloud-native representation will be done by the corresponding cloud mapper process.
For example, if the device is connected to Cumulocity IoT cloud platform, the Cumulocity cloud mapper process will translate the thin-edge alarm JSON data to its equivalent Cumulocity SmartREST representation.

:::info
As of now, alarm data mapping is supported only on Cumulocity IoT cloud platform.
:::

### Cumulocity cloud data mapping

The Cumulocity mapper will convert Thin Edge JSON alarm into Cumulocity SmartREST messages and send it to Cumulocity via MQTT.

For example the `temperature_high` alarm with `critical` severity described in the earlier sections will be converted to the following Cumulocity SmartREST message:

```csv
301,temperature_high,"Temperature is very high",2021-01-01T05:30:45+00:00
```

The message is published to the `c8y/s/us` topic which will get forwarded to the connected Cumulocity cloud instance.

If the alarm is raised from a child device, the payload is published to `c8y/s/us/<child-device-id>` topic instead.

If an alarm contains a `custom fragment` then, the alarm message will be converted to `cumulocity json`
format and then will be published on to `c8y/alarm/alarms/create` topic.

An example of the translated custom message for `thin-edge` device will be as below

```json
{
  "severity": "MAJOR",
  "type": "temperature_high",
  "time": "2023-01-25T18:41:14.776170774Z",
  "text": "Temperature High",
  "someOtherCustomFragment": {
    "nested": {
      "value": "extra info"
    }
  }
}
```

An example of the translated `cumulocity` alarm message for a `child` device with a `custom` fragment will be as below:

```json
{
  "severity": "MAJOR",
  "type": "pressure_alarm",
  "time": "2023-01-25T18:41:14.776170774Z",
  "text": "Pressure alarm",
  "someOtherCustomFragment": {
    "nested": {
      "value": "extra info"
    }
  },
  "externalSource": {
    "externalId": "child_device_id",
    "type": "c8y_Serial"
  }
}
```
Find more information about SmartREST representations for alarms in Cumulocity [here](https://cumulocity.com/guides/10.11.0/reference/smartrest-two/#alarm-templates).

Find more information about alarms data model in Cumulocity [here](https://cumulocity.com/guides/concepts/domain-model/#events).
