# How to use [`tedge mqtt`](../references/tedge-mqtt.md) pub and sub?

thin-edge.io cli provides a convenient way to debug and aid development process.

## Publish

Command [`tedge mqtt pub`](../references/tedge-mqtt.md) can be used to publish MQTT messages on a topic to the local mosquitto server.

Example:

```shell
tedge mqtt pub 'tedge/measurements' '{​​​​ "temperature": 21.3 }'​​​
```

`tedge mqtt pub` supports setting of QoS for MQTT messages:

```shell
tedge mqtt pub 'tedge/measurements' '{​​​​ "temperature": 21.3 }' --qos 2
```

## Subscribe

Command [`tedge mqtt sub`](../references/tedge-mqtt.md) can be used to ease debugging of of MQTT communication on local bridge. You can subscribe to topic of your choosing:

```shell
tedge mqtt sub tedge/errors
```

Or you can subscribe to any topic on the server using wildcard (`#`) topic:

```shell
tedge mqtt sub '#'
```

Now use `tedge mqtt pub 'tedge/measurements' '{​​​​ "temperature": 21.3 }'` to publish message on `tedge/measurements` topic with payload `{​​​​ "temperature": 21.3 }`.

All messages from sub command are printed to `stdout` and can be captured to a file if you need to:

```shell
tedge mqtt sub '#' > filename.mqtt
```

Wildcard (`#`) topic is used by [MQTT protocol](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901242) as a wildcard and will listen on all topics
