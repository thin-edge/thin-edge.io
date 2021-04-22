# The tedge-mapper

The tedge-mapper is a key concept to support multiple cloud providers.
The purpose is to translate
messages written using the cloud-agnostic [Thin Edge JSON format](thin-edge-json.md),
into cloud-specific messages.

tedge-mapper is composed of multiple cloud-specific mappers, such as Cumulocity mapper and Azure mapper.
Each mapper is responsible for its dedicated cloud.
> Note: The tedge-mapper contains the Cumulocity mapper only currently.

A mapper subscribes the reserved MQTT topic `tedge/measurements` with the QoS level At Least Once.
The messages arrived in the mapper should be formed in the [Thin Edge JSON](thin-edge-json.md) format. 
The mapper verifies whether the arrived messages are correctly formatted,
in case the verification fails, the mapper publishes a corresponded error message
on the topic `tedge/errors` with the QoS level At Least Once.

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

Once the mapper receives a correctly formatted message, 
the message will be translated into a cloud specific format.

## Cumulocity mapper
The Cumulocity mapper translates [Thin Edge JSON](thin-edge-json.md) into Cumulocity's [JSON via MQTT](https://cumulocity.com/guides/device-sdk/mqtt/#json).
Translated messages will be published in the topic `c8y/measurement/measurements/create`.

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
3. Another `temperature` is added.

Going through 1), the `type` is a mandatory field in the Cumulocity's JSON via MQTT manner,
therefore, the Cumulocity mapper always adds `ThinEdgeMeasurement` as a type.
This value is not configurable by user.

Next, 2) `time` will be added by the mapper **only when it is not specified in a received Thin Edge JSON message**.
In this case, the timezone is always UTC+0. If you want other timezone, specify the time filed in Thin Edge JSON.

The last 3), the mapper uses a measurement name ("temperature" in this example)
as both a fragment type and a fragment series in [Cumulocity's measurements](https://cumulocity.com/guides/reference/measurements/#examples).

After the mapper publishes a message on the topic `c8y/measurement/measurements/create`,
the message will be transferred to the topic `measurement/measurements/create` by [MQTT bridge](../references/bridged-topics.md).

## Topics used by tedge-mapper
- Incoming topics
    - `tedge/measurements`

- Outgoing topics
    - `tedge/errors` (for errors)
    - `c8y/measurement/measurements/create` (for Cumulocity)
