---
title: Service Commands
tags: [Reference, Agent, Services]
sidebar_position: 8
description: Acting on the services of a device over MQTT
---

# Service Commands

A *service command* acts on one service of a device: starting it, stopping it, restarting it,
or any other action that service supports.

- A service declares the actions it supports in the metadata of its `control` command.
- An action is triggered as a `control` command naming that action.
- `tedge-agent` executes the commands of the services registered with its own device as their `@parent`.
- The shipped `control` workflow delegates every action to [`tedge service`](../../cli/tedge-service),
  which runs the action through one of two [backends](../../service-plugin-api#backends),
  the init system or a service plugin.

The [`restart` operation](../restart-operation) restarts a device;
the `restart` action of a service restarts one service of that device.

## Control actions {#actions}

A `control` command `action` name matches `[A-Za-z][A-Za-z0-9_-]*`:
letters, digits, `_` and `-`, starting with a letter.
Action names are case-sensitive, so `requestLog` and `requestlog` are two actions.

## MQTT API

The service command API follows the [generic %%te%% rules for operations](../device-management-api),
applied to a service topic identifier such as `device/main/service/nodered`
and to the `control` operation.

### Declaring the control actions

A service declares its control actions by publishing a retained message on `te/<service-topic-id>/cmd/control`,
with the action names in an `actions` array.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/control' '{"actions": ["restart", "pause"]}'
```

To remove one action, the service publishes the message again without that action.

### Clearing the control actions

A service removes all its actions by publishing a retained empty message on the same topic.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/control' ''
```

### Triggering a control action

A `control` command is published on `te/<service-topic-id>/cmd/control/<cmd_id>`,
with the `action`, starting in the `init` state and ending in `successful` or `failed`.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/control/c8y-mapper-123' '{
    "status": "init",
    "action": "restart",
    "serviceName": "nodered",
    "serviceType": "service"
}'
```

| Field         | Description                                                                        |
|---------------|------------------------------------------------------------------------------------|
| `status`      | The state of the command, as for every %%te%% command                              |
| `action`      | The action to run, one of the actions the service declares                         |
| `serviceName` | The name of the service, as the backend running the action knows it                |
| `serviceType` | The type of the service, which selects the backend that runs the action            |

Progress is published on the same topic by whoever executes the command,
and the command is cleared by the requester.

```sh te2mqtt formats=v1
tedge mqtt pub --retain 'te/device/main/service/nodered/cmd/control/c8y-mapper-123' '{
    "status": "failed",
    "action": "restart",
    "serviceName": "nodered",
    "serviceType": "service",
    "reason": "The action failed, see the operation log for the reason"
}'
```

## Shipped workflow

A %%te%% service managed by an init unit of its own declares `restart` when it starts.

`tedge-agent` installs the workflow of the `control` command in `/etc/tedge/operations/control.toml`.
It declares `type = "service"`, so it applies to service entities.
See [scoping a workflow by entity type](../operation-workflow#workflow-entity-type).

The workflow runs every action through `tedge service`, custom actions included,
so a custom action only has to be supported by the init system or by the service plugin.

## From the cloud

A Cumulocity operator triggers these actions from the service itself.
See [service commands with Cumulocity](../../../operate/c8y/service-commands).
