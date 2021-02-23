# Send Thin Edge JSON data

Once your thin edge device is configured and connected to an IoT cloud provider, you can start sending measurements.
Refer to [Connecting to Cumulocity](../tutorials/connect-c8y.md) or tutorials for other cloud providers 
to learn how to connect your Thin Edge device to an IoT cloud provider. 

In this tutorial, we'll see how different kinds measurements are represented in Thin Edge JSON format and 
how they can be sent to the connected cloud provider. 
For a more detailed specification of this data format, refer to [Thin Edge JSON Specification](../architecture/thin-edge-json.md)

## Sending measurements

A simple single-valued measurement like a temperature measurement, can be represented in Thin Edge JSON as follows:

```json
{ "temperature": 25 }
```

with the key-value pair representing the measurement type and the numeric value of the measurement.

This measurement can be sent from the Thin Edge device to the cloud by publishing this message to the **tedge/measurements** MQTT topic.
In this tutorial, we'll be using the `tedge mqtt pub` commandline utility to send any data to the Thin Edge MQTT broker.

The temperature measurement described above can be sent using the `tedge mqtt pub` command as follows:

```shell
$ tedge mqtt pub tedge/measurements '{ "temperature": 25 }'
```

The first argument to the `tedge mqtt pub` command is the topic to which the measurements must be published to.
The second argument is the Thin Edge JSON representation of the measurement itself.

## Complex measurements

You can represent measurements that are far more complex than the single-valued ones described above using the Thin Edge JSON format.

A multi-valued measurement like `coordinate` that consists of `x`, `y` and `z` coordinate values can be represented as follows:

```json
{
  "coordinate": {
    "x": 32.54,
    "y": -117.67,
    "z": 98.6
  }
}
```

Here is another complex message consisting of single-valued measurements: `temperature` and `pressure` 
along with a multi-valued `coordinate` measurement, all sharing a single timestamp captured as `time`.

```json
{
  "time": "2020-10-15T05:30:47+00:00",
  "temperature": 25,
  "coordinate": {
    "x": 32.54,
    "y": -117.67,
    "z": 98.6
  },
  "pressure": 98
}
```

Refer to [Thin Edge JSON Specification](../architecture/thin-edge-json.md) for more details on the kinds of telemetry 
data that can be represented in Thin Edge JSON format and the reserved fields like `time` used in the above example.

## Error detection

If the data published to the **tedge/measurements** topic are not valid Thin Edge JSON measurements, those won't be
sent to the cloud but instead you'll get a feedback on the **tedge/errors** topic, if you subscribe to it.
The error messages published to this topic will be highly verbose and may change in the future.
So, use it only for debugging purposes during the development phase and should **not** to be used for any automation.

You can use the `tedge mqtt sub` command to subscribe to the error topic as follows:

```shell
$ tedge mqtt pub tedge/errors
```
