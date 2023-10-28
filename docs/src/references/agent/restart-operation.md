---
title: Restart Operation
tags: [Reference, Agent, Restart]
sidebar_position: 4
---

# Restart Operation

Thin-edge defines a `restart` operation to restart a device, being the main device or a child device.

- A restart is typically triggered by a [mapper](../mappers/index.md) on behalf of a cloud operator.
- A restart can also be triggered from another operation (as a software update)
  or service (detecting for instance some anomalies requesting a reboot).
- `tedge-agent` is the reference implementation of the `restart` operation.
- However, a custom `restart` plugin implementation can be installed on a device with specific requirements.

## MQTT API

The `restart` operation API follows the [generic thin-edge rules for operations](./device-management-api.md):

- The `te/<device-topic-id>/cmd/restart` topic is used to tell the device `<device-topic-id>` can be restarted.
- Each `restart` request is given a `<command-id>` and a dedicated topic  `te/<device-topic-id>/cmd/restart/<command-id>`,
  where all the subsequent states of the restart command are published during its execution.
- The workflow is [generic with `"init"`, `"executing"`, `"successful"` and `"failed"` statuses](./device-management-api.md#operation-workflow).

### restart registration

The registration message of the `restart` operation on a device is an empty JSON object `{}`.

```sh te2mqtt formats="v1"
tedge mqtt pub --retain 'te/device/child001///cmd/restart' '{}'
```

### init state

To trigger a restart operation on a device, the requester has no information to provide.
It only has to create a new `restart` command instance topic.

```sh te2mqtt formats="v1"
tedge mqtt pub --retain 'te/device/child001///cmd/restart/c8y-2023-09-08T18:13:00' '{
    "status": "init"
}'
```

### executing state

When ready, but before actually restarting the device,
the agent or the `restart` plugin publishes the new state of the command.

```sh te2mqtt formats="v1"
tedge mqtt pub --retain 'te/device/child001///cmd/restart/c8y-2023-09-08T18:13:00' '{
    "status": "executing"
}'
```

### successful state

After a successful reboot,
the agent or the `restart` plugin publishes the new state of the command.

```sh te2mqtt formats="v1"
tedge mqtt pub --retain 'te/device/child001///cmd/restart/c8y-2023-09-08T18:13:00' '{
    "status": "successful"
}'
```

### failed state

In case the reboot failed for some reason,
the agent or the `restart` plugin publishes the new state of the command,
adding a `reason` text field with the error. 

```sh te2mqtt formats="v1"
tedge mqtt pub --retain 'te/device/child001///cmd/restart/c8y-2023-09-08T18:13:00' '{
    "status": "failed",
    "reason": "The device has not restarted within 5 minutes"
}'
```

### Command cleanup

As for all commands, the responsibility of closing a `restart` is on the requester.
This is done by publishing an empty retained message on the command topic.

```sh te2mqtt formats="v1"
tedge mqtt pub --retain 'te/device/child001///cmd/restart/c8y-2023-09-08T18:13:00' ''
```

## Implementation Contract

The `restart` agent state must survive a device restart.

- The `executing` state must be published *before* a reboot is scheduled.
- The `successful` state is published *after* the reboot when the agent resumes an ongoing restart command.
- The agent needs to differentiate between a simple process restart vs the actual device restart itself.
- A simple process restart is considered as a `failed` restart.

