---
title: Sending Events
tags: [Getting Started, Telemetry]
sidebar_position: 6
---

# Sending Events

Events on thin-edge.io can be used to trigger signals when some event happens in the system.
For example, a person entering a room or someone logging into a machine/website can all be represented as events.
Events are stateless and hence are processed as and when they occur.
They don't represent state but can be used to represent state changes.
An event can't be updated/cleared once its triggered, unlike alarms that are cleared explicitly after processing.

Every event is uniquely identified by its type.
If multiple events are raised for a given type, thin-edge.io will process them all separately in the order in which they were raised.

## Sending an event

An event can be triggered on thin-edge.io by sending an MQTT message in Thin Edge JSON format to certain MQTT topics.

The scheme of the topic to publish the event data is as follows:

```text title="Topic"
tedge/events/<event-type>
```

The payload format must be as follows:

```json title="Payload"
{
  "text": "<event text>",
  "time": "<Timestamp in ISO-8601 format>"
}
```

Here is a sample event triggered for a `login_event` event type:

```sh te2mqtt
tedge mqtt pub tedge/events/login_event '
{
  "text": "A user just logged in",
  "time": "2021-01-01T05:30:45+00:00"
}'
```

:::note
Both the `text` field and the `time` field are optional.
:::

When the `message` field is not provided, the `event-type` from the MQTT topic will be used as the message as well if the connected cloud mandates one.
When the `time` field is not provided, thin-edge.io will use the current system time as the `time` of the event.
When you want to skip both fields, use an empty payload to indicate the same.
There are no such restrictions on the `<event-type>` value.

### Sending events from child devices

Events for child devices can be sent by publishing the event payload to `tedge/events/<event-type>/<child-device-id>` topic,
where the `child-device-id` is the unique device id of the child device.
The event payload structure is the same, as described in the previous section.

## Cloud data mapping

If the device is connected to some supported IoT cloud platform, an event that is triggered locally on thin-edge.io will be forwarded to the connected cloud platform as well.
The mapping of thin-edge events data to its respective cloud-native representation will be done by the corresponding cloud mapper process.
For example, if the device is connected to Cumulocity IoT cloud platform, the Cumulocity cloud mapper process will translate the thin-edge event JSON data to its equivalent Cumulocity SmartREST representation.

:::caution
As of now, event data mapping is supported only on Cumulocity IoT cloud platform.
:::

### Cumulocity cloud data mapping

The Cumulocity mapper will convert Thin Edge JSON events into its Cumulocity SmartREST equivalent if the payload only contains either a `text` field or `time` field.

For example the `login_event` described in the earlier sections will be converted to the following Cumulocity SmartREST message:

```csv
400,login_event,"A user just logged in",2021-01-01T05:30:45+00:00
```

The message is published to the `c8y/s/us` topic which will get forwarded to the connected Cumulocity cloud instance.

If the event JSON payload contains fields other than `text` and `time`, or when the payload size is more than 16K irrespective of its contents, it will be converted to Cumulocity JSON format.

The Cumulocity JSON mapping of the same event would be as follows:

```json
{
  "type":"login_event",
  "text":"A user just logged in",
  "time":"2021-01-01T05:30:45+00:00",
  "externalSource":{
    "externalId":"<child-device-id>",
    "type":"c8y_Serial"
  }
}
```

:::note
Mapped events will be sent to Cumulocity via MQTT if the incoming Thin Edge JSON event payload size is less than 16K bytes. If higher, HTTP will be used.
:::

Find more information about events data model in Cumulocity [here](https://cumulocity.com/guides/concepts/domain-model/#events).

## Sending an event for a child/external device to the cloud

An event for a child/external device can be triggered on thin-edge.io by sending an MQTT message in Thin Edge JSON format to certain MQTT topics.

The scheme of the topic to publish the event data is as follows:

```sh title="Topic"
tedge/events/<event-type>/<child-device-id>
```

The payload format must be as follows:

```json title="Payload"
{
  "type":"<event type>",
  "text": "<event text>",
  "time": "<Timestamp in ISO-8601 format>"
}
```

Here is a sample event triggered for a `login_event` event type for the `external_sensor` child device:

Command to send the event from a external device as below:

```sh te2mqtt
tedge mqtt pub tedge/events/login_event/external_sensor '{
  "type":"login_event",
  "text":"A user just logged in",
  "time":"2021-01-01T05:30:45+00:00"
}'
```

### Mapping of events to cloud-specific data format

If the child/external device is connected to some supported IoT cloud platform, an event that is triggered locally on thin-edge.io will be forwarded to the connected cloud platform as well.
The mapping of thin-edge events data to its respective cloud-native representation will be done by the corresponding cloud mapper process.

#### Cumulocity cloud data mapping

The Cumulocity mapper will convert Thin Edge JSON events into its Cumulocity JSON equivalent and sends them to the Cumulocity cloud.

The translated payload will be in the below format.

```json
{
  "type": "login_event",
  "text": "A user just logged in",
  "time": "2021-01-01T05:30:45+00:00",
  "externalSource":{
    "externalId": "external_sensor",
    "type": "c8y_Serial"
  }
}
```
Here the `externalId` will be derived from the `child-device-id` of the `child device event topic`.
