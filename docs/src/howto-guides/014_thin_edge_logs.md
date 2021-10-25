# The thin-edge logs
This document describes how to find the logs on the thin-edge device.

# Mosquitto logs
Thin-edge uses the `Mosquitto` as the `mqtt broker` for local communication as well as to communicate with the cloud/s.
The `Mosquitto` logs can be found in `/var/log/mosquitto/mosquitto.log`.
`Mosquitto` captures and logs error, warning, notice, information, subscribe, unsubscribe messages.

# Telemetry services logs
The logs of the thin-edge telemtry services those send the telemetry to cloud can be found as below

## Tedge Cumulocity mapper
The logs of the telemetry service that sends the telemetry from thin-edge device to the Cumulocity cloud as
`journalctl -u tedge-mapper-c8y.service`

## Tedge Azure mapper
The logs of the telemetry service that sends the telemetry from thin-edge device to the Azure cloud as
`journalctl -u tedge-mapper-az.service`

# Device monitoring logs
The thin-edge device monitoring logs can be found as below

## Collectd logs
`Collectd` is used for monitoring the resource status of a thin-edge device.
Colelctd logs all the messages at `/var/log/syslog`
Finding the collectd specific logs in `/var/log/syslog` could be tricky,
So, the collectd specific logs can be found using the `journalctl` as below

`journalctl -u collectd.service`

## Collectd mapper logs
Can be accessed as below

`journalctl -u tedge-mapper-collectd.service`

# Software Management logs
Thin-edge software management agent service logs the messages at `/var/log/tedge/agent`
These logs are mainly plugin operation logs.

## Tedge Agent logs
can be accessed as below

`journalctl -u journalctl -u tedge-agent.service

## Tedge cumulocity sm mapper logs
can be accessed as below

`journalctl -u tedge-mapper-sm-c8y.service`
