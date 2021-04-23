# The tedge-mapper

The tedge-mapper is a key concept to support multiple cloud providers.
The purpose is to translate
messages written using the cloud-agnostic [Thin Edge JSON format](thin-edge-json.md),
into cloud-specific messages.

The tedge-mapper is composed of multiple cloud-specific mappers, such as Cumulocity mapper and Azure mapper.
Each mapper is responsible for its dedicated cloud.
These specific mappers are launched by the respective `tedge connect` command.
For instance, `tedge connect c8y` establishes a bridge to Cumulocity and launches a Cumulocity mapper
that translates the messages in the background.

A mapper subscribes to the reserved MQTT topic `tedge/measurements` with the QoS level 1 (at least once).
The messages that arrive in the mapper should be formed in the [Thin Edge JSON](thin-edge-json.md) format.
The mapper verifies whether the arrived messages are correctly formatted,
in case the verification fails, the mapper publishes a corresponded error message
on the topic `tedge/errors` with the QoS level 1 (at least once).

When the mapper receives a correctly formatted message, 
the message will be translated into a cloud-specific format.

## Cumulocity mapper

The Cumulocity mapper translates [Thin Edge JSON](thin-edge-json.md) into Cumulocity's [JSON via MQTT](https://cumulocity.com/guides/device-sdk/mqtt/#json).
The translated messages are published on the topic `c8y/measurement/measurements/create` from where they are forwarded to Cumulocity.
This mapper is launched by the `tedge connect c8y` command, and stopped by the `tedge disconnect c8y` command.

Example in Thin Edge JSON:

```json
{
	"temperature": 23
}
```

Translated into JSON via MQTT by the Cumulocity mapper:

```json
{
	"type": "ThinEdgeMeasurement",
	"time": "2021-04-22T17:05:26.958340390+00:00",
	"temperature": {
		"temperature": {
			"value": 23
		}
	}
}
```

You can see the Cumulocity mapper added the three things which are not defined before translation.
1. `type` is added.
2. `time` is added.
3. Another hierarchy level is added, as required by the cumulocity data model.
String `temperature` is used as fragment and series.

(1) The `type` is a mandatory field in the Cumulocity's JSON via MQTT manner,
therefore, the Cumulocity mapper always adds `ThinEdgeMeasurement` as a type.
This value is not configurable by users.

(2) `time` will be added by the mapper **only when it is not specified in a received Thin Edge JSON message**.
In this case, the mapper uses the device's local timezone. If you want another timezone, specify the time filed in Thin Edge JSON.

(3) The mapper uses a measurement name ("temperature" in this example)
as both a fragment type and a fragment series in [Cumulocity's measurements](https://cumulocity.com/guides/reference/measurements/#examples).

After the mapper publishes a message on the topic `c8y/measurement/measurements/create`,
the message will be transferred to the topic `measurement/measurements/create` by [the MQTT bridge](../references/bridged-topics.md).

## Error cases

When some error occurs in a mapper process, the mapper publishes a corresponded error message
on the topic `tedge/errors` with the QoS level 1 (at least once).

Here is an example if you publish invalid Thin Edge JSON messages on `tedge/measurements`:

```shell
$ tedge mqtt pub tedge/measurements '{"temperature": 23,"pressure": 220'
$ tedge mqtt pub tedge/measurements '{"temperature": 23,"time": 220}'
```

Then, you'll receive error messages from the mapper on the topic `tedge/errors`:

```shell
$ ./tedge mqtt sub tedge/errors
[tedge/errors] Invalid JSON: Unexpected end of JSON: {"temperature":23,"pressure":220
[tedge/errors] Not a timestamp: the time value must be an ISO8601 timestamp string in the YYYY-MM-DDThh:mm:ss.sss.±hh:mm format, not a number.
```

## Topics used by tedge-mapper

- Incoming topics
    - `tedge/measurements`

- Outgoing topics
    - `tedge/errors` (for errors)
    - `c8y/measurement/measurements/create` (for Cumulocity)
