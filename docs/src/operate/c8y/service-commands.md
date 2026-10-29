---
title: Service Commands
tags: [Operate, Cumulocity, Services]
description: Acting on the services of a device from Cumulocity
---

import BrowserWindow from '@site/src/components/BrowserWindow';

A Cumulocity operator can perform operations on a service of a device:
start it, stop it, restart it, or run any other action that service supports.
This is the Cumulocity feature documented under
[service commands](https://cumulocity.com/docs/device-management-application/managing-device-services/#service-commands).

The mapping details are described in the
[Cumulocity mapper reference](../../../references/mappers/c8y-mapper#service-command).

## Declaring the supported service commands

A service is first registered with its service type in the `type` field.
The service type selects whether the init system or a [service plugin](../../../references/service-plugin-api) runs the actions.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/collectd' '{"@type":"service","@parent":"device/main//","type":"service","name":"collectd"}'
```

If [auto-registration](../../entity-management/auto-registration) is enabled,
a service that is not registered yet is registered with the default type `service`
when it declares its actions.

A service declares the actions that it supports
in the `actions` array of a retained message on its `cmd/control` topic.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/collectd/cmd/control' '{"actions":["restart","pause"]}'
```

The declared actions are copied, with their case and order,
to the `c8y_SupportedServiceCommands` fragment of the service managed object.
A custom action such as `pause` is declared the same way as a standard one.

An action whose name breaks the
[action name rule](../../../references/agent/service-commands#actions)
is not declared to Cumulocity at all, and the mapper logs why.

## Creating a service command operation

Create a service command operation from **Services** > *service name* > **Commands**.

<BrowserWindow url="https://example.cumulocity.com/apps/devicemanagement/index.html#/service/12345/commands">

![Cumulocity service commands](../../images/c8y-service-commands_dm.png)

</BrowserWindow>

The operation is sent to the device,
where `tedge-agent` runs the action on the service with the [`control` workflow](../../../references/agent/service-commands#shipped-workflow).
The operation moves to `SUCCESSFUL` or `FAILED` when the workflow ends.

### Rejected operations

An operation is failed straight away, with no %%te%% command published, when:

- the target service cannot be resolved.
- the action is not one the service has declared.
  The action name is compared with its case, so `RESTART` does not match a declared `restart`.
- the operation's `serviceName` or `serviceType` breaks the
  [rule for that argument](../../../references/service-plugin-api#validating-the-arguments).

## Removing an action

An action is removed by publishing the `cmd/control` message again without that action.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/collectd/cmd/control' '{"actions":["restart"]}'
```

The action is removed from the `c8y_SupportedServiceCommands` fragment of the service managed object.

Clearing the `cmd/control` topic with a retained empty message removes all the actions.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/collectd/cmd/control' ''
```

## Disabling service commands

To disable service commands from Cumulocity,
set `c8y.enable.service_commands` to `false` and restart the mapper.

```sh
tedge config set c8y.enable.service_commands false
```

```sh
sudo systemctl restart tedge-mapper-c8y
```

The mapper then ignores the `cmd/control` capabilities of the services and the `c8y_ServiceCommand` operations.

:::note
Disabling service commands keeps what the mapper created while they were enabled:
the `c8y_ServiceCommand` operation file under `/etc/tedge/operations/c8y/<service-external-id>`,
and the `c8y_SupportedServiceCommands` fragment of the service managed object.
:::
