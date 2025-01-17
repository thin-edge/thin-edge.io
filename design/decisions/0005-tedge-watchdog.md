# tedge-watchdog

* Date: __2024-10-18__
* Status: __Approved__

## Background

The **tedge-watchdog** component is to provide an interface to the [systemd watchdog functionality](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html) by using the thin-edge.io MQTT health status topic to publish messages to the systemd watchdog mechanism (which uses [sd_notify](https://www.freedesktop.org/software/systemd/man/latest/sd_notify.html#)).

The **tedge-watchdog** service, will send out periodic messages on the the `te/<service-topic-id>/cmd/health/check` topic, and wait for a response on the `te/<service-topic-id>/status/health` topic. If an updated response IS received, then the **tedge-watchdog** will send an `sd_notify` event on behalf of the service so that systemd knows that the service is still "ok/healthy".

Whilst the utilizing the systemd watchdog functionality makes for a low-resource usage solution, it has the following disadvantages:

* It can only monitor systemd services
* It relies on the `health/check` status being representative of being "healthy"
* It only works for systemd services
* It does not work for 3rd party devices which can't publish a `health/check` MQTT message themselves (e.g. mosquitto is not capable of doing this itself)
* It does not detect unexpected issues like constant high CPU load or exhausting the available File Descriptors
* It can not combine multiple status signals for one functional check (as systemd operates on a per service basis)


In production, devices in the field need some protection against technical and non-technical problems. For example, a service not responding would be a technical problem that can be overcome by this watchdog mechanism, however if a user accidentally stops and disables the **tedge-mapper-c8y** service, then the systemd mechanism will not be able to magically enable and restart it. Whilst the latter example may be extreme, it is unfortunately a reality for systems managed by humans. Therefore, in productive systems, a watchdog/monitoring system needs to account for both types of problems.


## Proposal

Tools such as [monit](https://mmonit.com/monit/) offer a flexible way for users to provide a functional-level monitoring/watchdog mechanism. Users can write rules to detect error scenarios, and run some corrective action to try and return the system to a functional state. In additional, monit can utilize thin-edge.io's telemetry API to publish events when such error scenarios are detected to provide feedback to system maintainers in the cloud that some devices are problems. For example, an event can be sent if the disk usage is too high, the File Descriptors are exhausted, the memory is too high. All of these metrics are only sent whilst in the error condition which means that it does not consume unnecessary network bandwidth sending the disk usage or memory usage to the cloud which incurs cost due to increase data volume usage, and cloud storage costs.

### Advantages

Using monit for service monitoring as the following benefits:

* Easy integration with thin-edge.io to send telemetry data on demand
* Works on any init system
* Relatively low memory footprint (written in C)
* Independent from the thin-edge.io development lifecycle (reduce risk due to shared code-based bugs)
* Can monitor any 3rd party components (e.g. mosquitto)
* Includes system monitoring (CPU, memory, disk space)
* Allows users to write their own rules

A community plugin, [tedge-monit-setup](https://github.com/thin-edge/tedge-monit-setup), has already been created which as some default rules out-of-the-box to monitor thin-edge.io's cloud connections and perform a reconnect if needed. The user can add their own rules to the installation (and even deploy the new rules using thin-edge.io), to focus on the components which are critical for their use-case. This project was originally created due to a bug observed with mosquitto which lead to the mosquitto bridge disconnecting and requiring a service restart before it could connect to the cloud again.


### Impact

The thin-edge.io project would be impacted by this decision in the following ways:

* stop further development of the **tedge-watchdog** component
* thin-edge.io focuses on directives that can be called by other tooling, e.g. `tedge connect c8y --test` to provide insight into whether it is working as expected or not
* add additional rules to the [tedge-monit-setup](https://github.com/thin-edge/tedge-monit-setup) project when new error scenarios are detected
