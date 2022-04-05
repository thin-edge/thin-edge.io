# How to enable the watchdog feature using systemd in thin-edge.io

When thin-edge.io services are managed using the systemd. Then one can use `systemd`
feature to check the health of these services as well. This document provides
information about how to use `systemd` for cheacking the health of the services.

Enabling the `watchdog` feature for a `thin-edge.io` service (tedge_agent, tedge_mapper_c8y/az/collectd) using the `systemd` is a two step process.

## Step 1: Enable the `watchdog` feature in the `systemd` service file
For example to enable the `watchdog` feature for `tedge-mapper-c8y` service, update systemd service file as shown below.

Add `tedge_watchdog.service` in  `After` under `[Unit]` section.
Add `WatchdogSec=5` under `[Service]` section.

The sample service file after updating looks as below.

```shell
[Unit]
Description=tedge-mapper-c8y converts Thin Edge JSON measurements to Cumulocity JSON format.
After=syslog.target network.target mosquitto.service tedge_watchdog.service

[Service]
User=tedge-mapper
ExecStart=/usr/bin/tedge_mapper c8y
Restart=on-failure
RestartPreventExitStatus=255
WatchdogSec=5
```

> Note: The systemd service file usually present in `/lib/systemd/system/tedge-mapper-c8y.service`.

## Step 2: Start the `tedge-watchdog` service

Start the `watchdog` service as below.
```shell
systemctl start tedge-watchdog.service
```

Now the `tedge-watchdog` service will be keep sending health check messages for every `WatchdogSec/2` seconds.
Once the response is received from the particular service, the `watchdog` service will send the notification
to the systemd on behalf of the service.

## Debugging
One can observe the message exchange between the `service` and the `watchdog` by subscribing to `tedge/health/#` and `tedge/health-check/#` topics.
For more info check [here](./020_monitor_tedge_health)

> Note: If the watchdog service did not send the notification to the systemd within `WatchdogSec`, then the systemd will kill the existing service process and restarts it.

> Note: [Here](https://www.medo64.com/2019/01/systemd-watchdog-for-any-service/) is an example about using `systemd watchdog` feature.