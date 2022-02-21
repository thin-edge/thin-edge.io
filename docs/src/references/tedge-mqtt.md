# The `tedge mqtt` command

```
tedge-mqtt 0.5.3
Publish a message on a topic and subscribe a topic

USAGE:
    tedge mqtt <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    help    Prints this message or the help of the given subcommand(s)
    pub     Publish a MQTT message on a topic
    sub     Subscribe a MQTT topic
```

## Pub

```
tedge-mqtt-pub 0.5.3
Publish a MQTT message on a topic

USAGE:
    tedge mqtt pub [FLAGS] [OPTIONS] <topic> <message>

FLAGS:
    -h, --help       Prints help information
    -r, --retain     Retain flag
    -V, --version    Prints version information

OPTIONS:
    -q, --qos <qos>    QoS level (0, 1, 2) [default: 0]

ARGS:
    <topic>      Topic to publish
    <message>    Message to publish
```

## Sub

```
tedge-mqtt-sub 0.5.3
Subscribe a MQTT topic

USAGE:
    tedge mqtt sub [FLAGS] [OPTIONS] <topic>

FLAGS:
    -h, --help        Prints help information
        --no-topic    Avoid printing the message topics on the console
    -V, --version     Prints version information

OPTIONS:
    -q, --qos <qos>    QoS level (0, 1, 2) [default: 0]

ARGS:
    <topic>    Topic to subscribe to
```
