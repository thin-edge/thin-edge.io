# Enabling systemd watchdog for thin-edge services

## Introduction

The systemd watchdog feature enables systemd to detect when a service is unhealthy or unresponsive and 
attempt to fix it by restarting that service.
To detect if a service is healthy or not, systemd relies on periodic health notifications from that service at regular intervals.
If the service fails to send that notification within a time threshold,
then systemd will assume that service to be unhealthy and restart it.

This document describes how the systemd watchdog mechanism can be enabled for thin-edge services.

## Enabling the systemd watchdog feature for a tedge service

Enabling systemd watchdog for a `thin-edge.io` service (tedge-agent, tedge-mapper-c8y/az/collectd) is a two-step process.

### Step 1: Enable the watchdog feature in the systemd service file

For example, to enable the watchdog feature for `tedge-mapper-c8y` service,
update the systemd service file as shown below:

> Note: The systemd service file for tedge services are usually present in `/lib/systemd/system` directory, 
> like `/lib/systemd/system/tedge-mapper-c8y.service`.

Add `tedge-watchdog.service` as an `After` service dependency under `[Unit]` section.
Add the watchdog interval as `WatchdogSec=30` under `[Service]` section.
Update the restart condition as `Restart=always` under `[Service]` section.

Here is the updated service file for `tedge-mapper-c8y` service:

```shell
[Unit]
Description=tedge-mapper-c8y converts Thin Edge JSON measurements to Cumulocity JSON format.
After=syslog.target network.target mosquitto.service tedge-watchdog.service

[Service]
User=tedge-mapper
ExecStart=/usr/bin/tedge_mapper c8y
Restart=always
RestartPreventExitStatus=255
WatchdogSec=30
```

### Step 2: Start the `tedge-watchdog` service

The `tedge-watchdog` service is responsible for periodically checking the health of
all tedge services for which the watchdog feature is enabled,
and send systemd watchdog notifications on their behalf to systemd.

Start and enable the `tedge-watchdog` service as follows:
	
```shell
systemctl start tedge-watchdog.service
systemctl enable tedge-watchdog.service
``` 

Once started, the `tedge-watchdog` service will keep checking the health of the monitored tedge services
by periodically sending health check messages to them within their configured `WatchdogSec` interval.

The health check request for service is published to `tedge/health-check/<service-name>` topic and
the health status response from that service is expected on `tedge/health/<service-name>` topic.

Once the health status response is received from a particular service,
the `tedge-watchdog` service will send the [systemd notification](https://www.freedesktop.org/software/systemd/man/sd_notify.html#) to systemd on behalf of that monitored service.

## Debugging

One can observe the message exchange between the `service` and the `watchdog`
by subscribing to `tedge/health/#` and `tedge/health-check/#` topics.
For more info check [here](./020_monitor_tedge_health.md)

> Note: If the watchdog service does not send the notification to the systemd within `WatchdogSec` interval for a service,
> then systemd restarts that service by killing the old process and spawning a new one to replace it.

> Note: [Here](https://www.medo64.com/2019/01/systemd-watchdog-for-any-service/) is an example about using `systemd watchdog` feature.
