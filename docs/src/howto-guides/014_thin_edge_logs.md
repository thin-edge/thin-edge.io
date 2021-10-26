# The thin-edge logs
This document describes how and where to find thin-edge logs on the device.

# Mosquitto logs
Thin-edge uses `Mosquitto` as the `mqtt broker` for local communication as well as to communicate with the cloud/s.
The `Mosquitto` logs can be found in `/var/log/mosquitto/mosquitto.log`.
`Mosquitto` captures error, warning, notice, information, subscribe, and unsubscribe messages.

# Telemetry services logs
The logs of the thin-edge telemtry services that send the telemetry data to cloud can be accessed as below

## Tedge Cumulocity mapper
The logs of the telemetry service that sends the telemetry data from thin-edge device to the `Cumulocity`
cloud can be accessed as below

`journalctl -u tedge-mapper-c8y.service`

## Tedge Azure mapper
The logs of the telemetry service that sends the telemetry data from thin-edge device to the `Azure` cloud as below

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
This service sends the monitoring data to the cloud, the logs can be accessed as below

`journalctl -u tedge-mapper-collectd.service`

# Software Management logs
This section describes how to access the software management services logs

## Software update logs
The software update logs can be found at `/var/log/tedge/agent`

## Tedge Agent logs
The agent service logs can be accessed as below

`journalctl -u journalctl -u tedge-agent.service`

## Tedge cumulocity sm mapper logs
The software management mapper service logs can be accessed as below

`journalctl -u tedge-mapper-sm-c8y.service`
