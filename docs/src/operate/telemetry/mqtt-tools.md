---
title: MQTT Tools
tags: [Operate, MQTT]
description: Publish and subscribe to MQTT messages on the command line
---

%%te%% cli provides a convenient way to debug and aid development process.

## Publish

Command [`tedge mqtt pub`](../../references/cli/tedge-mqtt.md) can be used to publish MQTT messages on a topic to the local mosquitto server.

Example:

```sh te2mqtt formats=v1
tedge mqtt pub 'te/device/main///m/env_sensor' '{"temperature": 21.3}'
```

Messages can also be published with a different Quality of Service (QoS).

```sh te2mqtt formats=v1
tedge mqtt pub 'te/device/main///m/env_sensor' '{"temperature": 21.3}' --qos 2
```

MQTT messages can also be published using the retained option which means that the message will be received by new MQTT clients connecting to the broker after the message was published.

Below shows an example of publishing a retained MQTT message:

```sh te2mqtt formats=v1
tedge mqtt pub --retain --qos 1 te/device/main///a/high_temperature '{
    "text": "Temperature is critical",
    "severity": "critical"
}'
```

:::note
By default the mqtt message will be published with retain flag set to false.
:::


## Subscribe

Command [`tedge mqtt sub`](../../references/cli/tedge-mqtt.md) can be used to ease debugging of of MQTT communication on local bridge. You can subscribe to topic of your choosing:

```sh te2mqtt formats=v1
tedge mqtt sub te/errors
```

Or you can subscribe to any topic on the server using wildcard (`#`) topic:

```sh te2mqtt formats=v1
tedge mqtt sub '#'
```

Now using a different console/shell, publish the following measurement so that the previous subscription will receive it:

```sh te2mqtt formats=v1
tedge mqtt pub --retain --qos 1 te/device/main///m/env_sensor '{"temperature": 21.3}'
```

All messages from sub command are printed to `stdout` and can be captured to a file if you need to:

```sh te2mqtt formats=v1
tedge mqtt sub '#' > filename.mqtt
```

Wildcard (`#`) topic is used by [MQTT protocol](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901242) as a wildcard and will listen on all topics
