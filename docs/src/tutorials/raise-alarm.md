# Thin Edge Alarm

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

`tedge/alarms/<severity>/<alarm-type>`

The payload format must be as follows:

```json
{
    "message": "<message text>",
    "time": "<Timestamp in ISO-8601 format>"
}
```

> Note: These messages must be sent with MQTT retained flag enabled and with QOS > 1 to ensure guaranteed processing by thin-edge.io.
Enabling the retained flag ensures that the alarm stays persisted with the MQTT broker until its state changes again.
These retained messages will make sure that the thin-edge.io processes or any other third-party processes subscribed to these alarms will get those, even if they were down at the moment the alarm was raised.
If multiple messages are sent to the same alarm topic, the last alarm is considered to have overwritten the previous one.

Here is a sample alarm raised for `temperature_high` alarm type with `critical` severity:

Topic: 
`tedge/alarms/critical/temperature_high`

Payload:
```json
{
    "message": "Temperature is very high",
    "time": "2021-01-01T05:30:45+00:00"
}
```

> Note: Both the `message` field and the `time` field are optional.
When a `message` is not provided, it is assumed to be empty.
When `time` is not provided, thin-edge.io will use the current system time as the `time` of the alarm.
When you want to skip both fields, use an empty json fragment `{}` as the payload to indicate the same.
An empty message can't be used for the same as empty messages are used to clear alarms, which is discussed in the next section.

The `<severity>` value in the MQTT topic can only be one of the following values:

1. critical
2. major
3. minor
4. warning

There are no such restrictions on the `<alarm-type>` value.

Thin-edge.io doesn't keep any history of all alarms raised on an alarm topic.

## Clearing alarms

An already raised alarm can be cleared by sending an empty message with retained flag enabled to the same alarm topic on which the original alarm was raised.

> Note: Using the retained flag is a must while clearing the alarm as well, without which the alarm won't be cleared properly.

If alarms of different severities exist for a given alarm type, they must all be cleared separately as they're all treated as independent alarms.

## Cloud data mapping

If the device is connected to some supported IoT cloud platform, any alarms raised locally on thin-edge.io will be forwarded to the connected cloud platform as well.
The mapping of thin-edge alarms data to its respective cloud-native representation will be done by the corresponding cloud mapper process.
For example, if the device is connected to Cumulocity IoT cloud platform, the Cumulocity cloud mapper process will translate the thin-edge alarm JSON data to its equivalent Cumulocity SmartREST representation.

> Warning: As of now, alarm data mapping is supported only on Cumulocity IoT cloud platform.
Find more information about alarms data model in Cumulocity [here](https://cumulocity.com/guides/concepts/domain-model/#events)
