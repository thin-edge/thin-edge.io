# Thin Edge Event

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

`tedge/events/<event-type>`

The payload format must be as follows:

```json
{
    "message": "<event message text>",
    "time": "<Timestamp in ISO-8601 format>"
}
```

Here is a sample event triggered for a `login_event` event type:

Topic: 
`tedge/events/login_event`

Payload:

```json
{
    "message": "A user just logged in",
    "time": "2021-01-01T05:30:45+00:00"
}
```

> Note: Both the `message` field and the `time` field are optional.

When the `message` field is not provided, the `event-type` from the MQTT topic will be used as the message as well if the connected cloud mandates one.
When the `time` field is not provided, thin-edge.io will use the current system time as the `time` of the event.
When you want to skip both fields, use an empty payload to indicate the same.
There are no such restrictions on the `<event-type>` value.

## Cloud data mapping

If the device is connected to some supported IoT cloud platform, an event that is triggered locally on thin-edge.io will be forwarded to the connected cloud platform as well.
The mapping of thin-edge events data to its respective cloud-native representation will be done by the corresponding cloud mapper process.
For example, if the device is connected to Cumulocity IoT cloud platform, the Cumulocity cloud mapper process will translate the thin-edge event JSON data to its equivalent Cumulocity SmartREST representation.

> Warning: As of now, event data mapping is supported only on Cumulocity IoT cloud platform.

### Cumulocity cloud data mapping

The Cumulocity mapper will convert Thin Edge JSON into Cumulocity SmartREST messages and send it to Cumulocity via MQTT,
if the event payload size is less than 16K bytes.

For example the `login_event` described in the earlier sections will be converted to the following Cumulocity SmartREST message:

```csv
400,login_event,"A user just logged in",2021-01-01T05:30:45+00:00
```

... and is published to `c8y/s/us` topic which will get forwarded to the connected Cumulocity cloud instance.

If the event payload size is more than 16K, it will be converted to Cumulocity JSON format and sent over HTTP.
The Cumulocity JSON mapping of the same event would be as follows:

```json
{
    "type":"login_event",
    "text":"A user just logged in",
    "time":"2021-01-01T05:30:45+00:00",
    "source": {
        "id":"<c8y-device-id>"
    }
}
```

Find more information about events data model in Cumulocity [here](https://cumulocity.com/guides/concepts/domain-model/#events).
