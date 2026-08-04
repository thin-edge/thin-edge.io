---
title: Service Commands
tags: [Reference, Agent, Services]
sidebar_position: 8
description: Acting on the services of a device over MQTT
---

# Service Commands

A *service command* acts on one service of a device: starting it, stopping it, restarting it,
or any other action that service supports.

- Each action a service supports is declared as a capability on its own topic.
- A command is then triggered on that topic, as any other %%te%% command.
- `tedge-agent` executes the commands of the services of *its own* device,
  delegating the actual work to [`tedge service`](../cli/tedge-service.md),
  which acts either through the init system or through a
  [service plugin](../service-plugin-api.md).

This is not the same thing as restarting the device the service runs on.
The [`restart` operation](./restart-operation.md) restarts a device;
a `restart` service command restarts a single service of that device.

## Actions

An action is named by the topic it is declared on, and nowhere else.
The name is a single lowercase token, matching `[a-z][a-z0-9_-]*`:
lowercase letters, digits, `_` and `-`, starting with a letter, at most 64 characters.

MQTT topic names are case-sensitive and do accept spaces,
so this rule is a restriction %%te%% puts on itself, not one the protocol imposes.
A name such as `RESTART` or `do something` is rejected wherever it enters the system.
This is what lets an action name stay the same
from a cloud command name, to a topic segment, to the argument of a command line.

`start`, `stop`, `restart`, `enable` and `disable` are the standard actions,
the ones %%te%% ships a workflow for.
Any other name is a custom action, supported by whatever runs it.

## MQTT API

The service command API follows the [generic %%te%% rules for operations](./device-management-api.md),
applied to a service topic identifier such as `device/main/service/nodered`.

### Declaring an action

A service declares an action by publishing a retained empty JSON object `{}`
on `te/<service-topic-id>/cmd/<action>`, one topic per action.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/restart' '{}'
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/pause' '{}'
```

One topic per action is what gives each action its own workflow.

An action is removed by clearing its topic with a retained empty message.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/pause' ''
```

### Triggering an action

A command is published on `te/<service-topic-id>/cmd/<action>/<command-id>`,
with the usual `init` → `executing` → `successful` | `failed` states.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/restart/c8y-mapper-123' '{
    "status": "init",
    "serviceName": "nodered",
    "serviceType": "service"
}'
```

| Field         | Description                                                                        |
|---------------|------------------------------------------------------------------------------------|
| `status`      | The state of the command, as for every %%te%% command                              |
| `serviceName` | The name of the service, as the backend running the action knows it                |
| `serviceType` | The type of the service, which selects the backend that runs the action            |

`serviceName` is there because the name a backend knows is not always in the topic.
`serviceType` decides which backend runs the action:
the default type `service` is handled by the init system,
any other type by the service plugin of that name.

Progress is published on the same topic by whoever executes the command,
and the command is finally cleared by the requester.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/restart/c8y-mapper-123' '{
    "status": "failed",
    "serviceName": "nodered",
    "serviceType": "service",
    "reason": "This action is not supported for that type of service"
}'
```

## Who executes a service command

`tedge-agent` executes the commands of the services of the device it runs on, and only those.

- A service belongs to the device given as its `@parent` at registration,
  not to the device its topic looks like it names,
  so this holds under a custom topic scheme too.
- A child device running its own `tedge-agent` executes the commands of its own services,
  with its own configuration and its own backends.
  No agent acts on behalf of another device.

:::note
The agent reads the registration data
of the target from the [entity store](../../operate/entity-management/rest_api.md)
to decide whether a service belongs to its own device.
When that lookup fails, the agent cannot tell whether the command is its own,
so it leaves the command untouched rather than driving a state machine it may not own.
:::

## Shipped workflows

`tedge-agent` installs a workflow for each standard action in
`/etc/tedge/operations/service_start.toml`, `service_stop.toml`, `service_restart.toml`,
`service_enable.toml` and `service_disable.toml`.
Each declares `type = "service"`, which is what scopes it to service entities.
See [scoping a workflow by entity type](./operation-workflow.md#workflow-entity-type).

Each runs the action with:

```sh
sudo -n tedge service ${.topic.operation} ${.payload.serviceName} --service-type ${.payload.serviceType}
```

Exit code `0` moves the command to `successful`, any other code to `failed`,
with the reason given by the backend written to the operation log.
The step carries a timeout, so a backend that never returns
ends as a failed command rather than a stuck one:
180 seconds for `start`, `stop` and `restart`,
twice what systemd itself waits to stop and to start a unit,
and 60 seconds for `enable` and `disable`, which wait for no service to change state.

These five files are installed as *templates*:
each is created when missing, and never overwritten once it differs from the shipped copy.
An administrator can adapt a workflow, or delete it to disable the action altogether,
and an upgrade will not restore it.

Custom actions have no shipped workflow.
Define one as any other [user-defined workflow](./operation-workflow.md),
with `type = "service"`.

### Restarting tedge-agent

`tedge-agent` is what runs a service command, so restarting it is not a step like any other:
the process is killed in the middle of its own workflow.
The shipped `restart` workflow handles it with the self-restart pattern —
the agent persists the state awaiting its restart, then asks its runtime to stop,
and whatever runs the agent starts it again.
The command completes exactly once when the workflow resumes.

No backend is asked to restart the agent,
so this works whether the agent runs as a service of its own,
or is co-hosted with the mappers under `tedge run all`,
where no `tedge-agent` unit exists.

### Services that cannot be stopped

The shipped `stop` workflow refuses two cases,
both being components %%te%% needs in order to report the outcome of the command asking for it:

- **tedge-agent**, which runs the command.
  A stopped agent cannot report anything, and nothing would start it again.
- **any mapper connected to a cloud**, which carries the outcome to that cloud.
  A mapper is recognized by its name, always `tedge-mapper-<x>`,
  whatever its cloud, topic prefix and profile.

`tedge-mapper-collectd` and `tedge-mapper-local` are the exception, being connected to no cloud:
the collectd mapper feeds local measurements in, and the local mapper transforms data on the device.
Stopping one of them takes no way of reporting anything away,
so both are stopped as any other service.
Neither takes a cloud profile, so both always carry those exact names.

A refused command moves to `failed` with the reason naming why.

## From the cloud

A Cumulocity operator triggers these actions from the service itself.
See [service commands with Cumulocity](../../operate/c8y/service-commands.md).
