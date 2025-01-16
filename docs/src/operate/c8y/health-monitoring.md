---
title: Health Monitoring
tags: [Operate, Cumulocity, Monitoring]
description: Monitoring the health of services on devices
---

The health of a %%te%% service or any other `service` that is running on the %%te%% device
or on the `child` device can be monitored from the **Cumulocity** by sending the `health-status` message to **Cumulocity**.

## Publish health status

A health status message can be published for any service on a `status/health` channel. The health message should at least contain the `status` of the service.

:::note
The `status` here can be `up` or `down` or any other string. For example, `unknown`.
:::

For example, to update the health status of `device/my-device/service/my-test-service` service, one has to send the
following message:

```sh te2mqtt formats=v1
tedge mqtt pub te/device/my-device/service/my-test-service/status/health '{"status":"up"}' -q 2 -r
```

:::note
The health status message has to be sent as a *retain* message.
:::

When an empty health status message is sent, e.g. `{}` or `''`, the `status` will be replaced with `unknown`.

## Conversion of the health status message to Cumulocity service monitor message

The `tedge-mapper-c8y` will translate any health status message that is received on `te/+/+/+/+/status/health` topic to
Cumulocity [Service creation](https://cumulocity.com/docs/smartrest/mqtt-static-templates/#102) SmartREST message and
send it to the `Cumulocity` cloud. If a service was not previously registered and it fulfills the requirements for
auto-registration, it will be auto-registered as described [in the Auto Registration
section](https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#auto-registration).

For example, assuming a service `device/child1/service/service1`, running on a device `device/child1//`, which is a
child device of %%te%% device `device/main//` with an ID of `TE_DEVICE`, the resulting topic mapping looks like
this:

<div class="code-indent-left">

**%%te%% health status message**

```sh te2mqtt formats=v1
tedge mqtt pub te/device/child1/service/service1/status/health '{"status":"up"}' -q 2 -r
```

</div>

<div class="code-indent-right">

**Cumulocity (output)**

```text title="Topic"
c8y/s/us/<device-id>:device:child1
```

```text title="Payload"
102,<device-id>:device:child1:service:service1,service,service1,up
```

</div>

## Configuring the default service type

The default service type can be configured using the `tedge` cli.

The example below shows how one can set the default service type to `systemd`.

```sh
sudo tedge config set service.type systemd
```

:::note
When the `type` property was not included in the service registration message, then the configured default value
will be used by the mapper while auto-registering the service.
:::

To clear the configured default service type one can use the command below.
This will set the `service.type` to `service`.

```sh
sudo tedge config unset service.type
```

# References

More info about the service monitoring can be found in the link below.

[Service monitoring Cumulocity](https://cumulocity.com/docs/device-management-application/viewing-device-details/#services)
