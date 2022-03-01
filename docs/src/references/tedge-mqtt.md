# The `tedge mqtt` command

```
tedge-mqtt 
Publish a message on a topic and subscribe a topic

USAGE:
    tedge mqtt <SUBCOMMAND>

OPTIONS:
    -h, --help    Print help information

SUBCOMMANDS:
    help    Print this message or the help of the given subcommand(s)
    pub     Publish a MQTT message on a topic
    sub     Subscribe a MQTT topic
```

## Pub

```
tedge-mqtt-pub 
Publish a MQTT message on a topic

USAGE:
    tedge mqtt pub [OPTIONS] <TOPIC> <MESSAGE>

ARGS:
    <TOPIC>      Topic to publish
    <MESSAGE>    Message to publish

OPTIONS:
    -h, --help         Print help information
    -q, --qos <QOS>    QoS level (0, 1, 2) [default: 0]
    -r, --retain       Retain flag
```

## Sub

```
tedge-mqtt-sub 
Subscribe a MQTT topic

USAGE:
    tedge mqtt sub [OPTIONS] <TOPIC>

ARGS:
    <TOPIC>    Topic to subscribe to

OPTIONS:
    -h, --help         Print help information
        --no-topic     Avoid printing the message topics on the console
    -q, --qos <QOS>    QoS level (0, 1, 2) [default: 0]
```
