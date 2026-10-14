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
- `tedge-agent` executes the commands of the services registered with its own device as their `@parent`,
  running the workflow defined for that action.
- The shipped `restart` workflow delegates the work to [`tedge service`](../../cli/tedge-service),
  which runs the action through one of two [backends](../../service-plugin-api#backends),
  the init system or a service plugin.

The [`restart` operation](../restart-operation) restarts a device;
a `restart` service command restarts one service of that device.

## Actions

An action is declared as a command type of the service, the `<cmd_type>` segment of its topic.
The name is a single lowercase token, matching `[a-z][a-z0-9_-]*`:
lowercase letters, digits, `_` and `-`, starting with a letter.

## MQTT API

The service command API follows the [generic %%te%% rules for operations](../device-management-api),
applied to a service topic identifier such as `device/main/service/nodered`.

### Declaring an action

A service declares an action by publishing a retained empty JSON object `{}`
on `te/<service-topic-id>/cmd/<cmd_type>`, one topic per action.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/pause' '{}'
```

One topic per action gives each action its own workflow.

### Clearing an action

A service removes an action by publishing a retained empty message on its topic.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/pause' ''
```

### Triggering an action

A command is published on `te/<service-topic-id>/cmd/<cmd_type>/<cmd_id>`,
starting in the `init` state and ending in `successful` or `failed`.

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

Progress is published on the same topic by whoever executes the command,
and the command is cleared by the requester.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/restart/c8y-mapper-123' '{
    "status": "failed",
    "serviceName": "nodered",
    "serviceType": "service",
    "reason": "This action is not supported for that type of service"
}'
```

## Shipped workflows

A %%te%% service managed by an init unit of its own declares `restart` when it starts.

`tedge-agent` installs the workflow of the `restart` action
in `/etc/tedge/operations/service_restart.toml`.
It declares `type = "service"`, so it applies to service entities.
See [scoping a workflow by entity type](../operation-workflow#workflow-entity-type).
