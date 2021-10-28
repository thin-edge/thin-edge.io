# The thin-edge logs
The logs that are useful to debug thin-edge.io break down into logs that are created by thin-edge itself and third party components.

## Thin-edge component logs
The Thin-edge is composed of different components like mappers, agent, and plugins. The log messages of these components can be accessed as below.

### Cloud mapper logs
The Thin-edge cloud mapper services that send the measurement data to cloud can be accessed as below

#### Tedge Cumulocity mapper
The logs of the cumulocity mapper service that sends the measurement data from thin-edge device to the `Cumulocity`
cloud can be accessed as below

`journalctl -u tedge-mapper-c8y.service`

#### Tedge Azure mapper
The logs of the Azure mapper service that sends the measurement data from thin-edge device to the `Azure`
cloud can be accessed as below

`journalctl -u tedge-mapper-az.service`

### Device monitoring logs
The thin-edge device monitoring component logs can be found as below

#### Collectd mapper logs
This service sends the device monitoring data to the cloud, the logs can be accessed as below

`journalctl -u tedge-mapper-collectd.service`

### Software Management logs
This section describes how to access the software management component logs

#### Software update logs
The software update log is created per operation, and it can be found at `/var/log/tedge/agent`

#### Tedge Agent logs
The agent service logs can be accessed as below

`journalctl -u journalctl -u tedge-agent.service`

#### Tedge cumulocity sm mapper logs
The software management mapper service logs can be accessed as below

`journalctl -u tedge-mapper-sm-c8y.service`

## Thirdparty component logs
Thin-edge uses the third-party components `Mosquitto` mqtt broker and `Collectd`. The logs that are created by these components
can be accessed on a thin-edge device as below.

### Mosquitto logs
Thin-edge uses `Mosquitto` as the `mqtt broker` for local communication as well as to communicate with the cloud/s.
The `Mosquitto` logs can be found in `/var/log/mosquitto/mosquitto.log`.
`Mosquitto` captures error, warning, notice, information, subscribe, and unsubscribe messages.

### Collectd logs
`Collectd` is used for monitoring the resource status of a thin-edge device.
Colelctd logs all the messages at `/var/log/syslog`
Finding the collectd specific logs in `/var/log/syslog` could be tricky,
So, the collectd specific logs can be found using the `journalctl` as below

`journalctl -u collectd.service`


