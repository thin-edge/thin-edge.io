---
title: Operation Workflows
tags: [Reference, MQTT, Operations]
sidebar_position: 6
---

# Operation Workflows

Thin-edge provides the tools to define, extend and combine *operation workflows*
that rule the sequence of steps applied when a maintenance *operation* is triggered by an operator or some software component,
whether it is a *command* to restart the device, to update a configuration file or to install a new software.

An operation workflow defines the possible sequences of actions for an operation request
from its initialization up to its success or failure. It specifies the actions to perform
as well as any prerequisite checks, outcome validations and possible rollbacks.
However, a workflow doesn't define how to perform these actions.
These are delegated to software components participating in the operation progress.
More precisely, an operation workflow defines:
- the *observable states* of an ongoing operation instance
  from initialization up to a final success or failure
- the *participants* and their interactions, passing the baton to the software component
  which responsibility is to advance the operation in a given state
  and to notify the other participants what is the new resulting state
- the *possible state sequences* so the system can detect any stale or misbehaving operation request.

These workflows are extensible. An agent developer can:
- override existing workflows by replacing the components responsible for certain steps with new ones
- implement new components to handle the specificities of some action such as domain-specific checks
- define new states and tell the system which software component will handle them: a script, a unix daemon, an external device
- introduce new transitions such as rollbacks or conditional executions
- create new workflows, combining other workflows and steps

## Operations, Capabilities, and Commands

From a user perspective an *operation* is a predefined sequence of actions
that an operator can trigger on a device to reach some desirable state.
It can be to restart the device or to install some new software.
From an implementation perspective, an operation is an API identified by a well-known name such as `restart` or `software_update`.
This API rules the coordination among the software components that need to interact to advance the operation.

Not all entities and components of a thin-edge device support all the operations,
and, even if they do, the implementations might be specific.
Installing a software package on top of service makes no sense.
Restarting the device is not the same as restarting one of its services.
Each entity or component has to declare its *capabilities* i.e. the operations made available on this target.

Strictly speaking, capabilities are not implemented nor declared by the devices and the services themselves.
There are implemented by thin-edge services and plugins.
These are the components which actually implement the operations interacting with the operating system and other software.
For instance, device restart and software updates are implemented by the `tedge-agent`.

Once an operation has been registered as a capability of some target entity or component,
an operator can trigger operation requests a.k.a *commands*,
for this kind of operation on this target,
say to request a software update than a restart of the device.

## MQTT Topics

Operations, capabilities and commands are declared, triggered and managed using MQTT topics,
all built along the same schema, matching the topic filter `te/+/+/+/+/cmd/+/+`,
with a target prefix `te/+/+/+/+` and a command specific suffix `/cmd/+/+`:

| root   | target           | command keyword | operation name | command instance id |
|--------|------------------|-----------------|----------------|---------------------|
| __te__ | /*a*/*b*/*c*/*d* | /__cmd__        | /*operation*   | /*command-id*       |

The prefix __te__/*a*/*b*/*c*/*d* uniquely identifies the entity or component that is the target of commands.
It can be:
- the main device: `te/device/main//`
- a child device: `te/device/child-xyz//`
- a service: `te/device/main/service/tedge-agent`
- or any application specific entity identifier such as `te/raspberry-pi/123/process/collectd`.

The longer prefix __te__/*a*/*b*/*c*/*d*/__cmd__ groups all the capabilities and commands
related to the entity identified by __te__/*a*/*b*/*c*/*d*.

### Capabilities

A capability, the ability for an entity __te__/*a*/*b*/*c*/*d* to handle a given *operation*, is published as a retained message
on the topic __te__/*a*/*b*/*c*/*d*/__cmd__/*operation*, in which the suffix is the well-known name of the operation.

One can subscribe to the following topic to get all the capabilities of a thin-edge device and its child-devices and services.

```sh te2mqtt
tedge mqtt sub 'te/+/+/+/+/cmd/+' 
```

The retained messages published on these topics are operation specific and defined by the operation APIs.
They provide operation specific parameters such as the list of software package types that can be installed,
or the list of file types that configured.

As an example, the `tedge-agent` which implements the `restart` and `software_update` capabilities for the main device,
will emit two retained messages.

A first message to tell that the main device can be restarted:

```sh te2mqtt
tedge mqtt pub -r 'te/device/main///cmd/restart' '{}' 
```

A second one to tell that debian packages can be installed on the main device: 

```sh te2mqtt
tedge mqtt pub -r 'te/device/main///cmd/software_update' '{ "type": ["apt"] }' 
```

### Commands

The topics matching __te__/*a*/*b*/*c*/*d*/__cmd__/*operation*/*command-id* are used to trigger and manage commands,
i.e. operation requests on a specific target for a specific *operation*.

Each request is given a unique command identifier.
Combined with the target identifier and the operation name this defines a request specific topic
where the current state of the command workflow is published as a retained message.
This unique id assigned by the requester, who is also responsible for creating the topic
with an initial state and for finally removing it.

As an example, software update is an operation that requires coordination between a mapper and `tedge-agent`.
On reception of a software update request from the cloud operator,
the `tedge-mapper` creates a fresh new topic for this command,
say `te/device/main///cmd/software_update/c8y-mapper-123` for the 123<sup>rd</sup> request.
On this topic, a first retained messages is published to describe the operator expectations for the software updates.

```sh te2mqtt
tedge mqtt pub -r 'te/device/main///cmd/software_update/c8y-mapper-123' '{
    "status": "init",
    "modules": [
        {
            "type": "apt",
            "name": "collectd",
            "version": "5.7",
            "action": "install"
        }
    ]
}' 
```

Then, the `tedge-agent` and possibly other software components take in charge the command,
making it advance to some final state,
publishing all the successive states as retained messages on the command topic.

Eventually, the `tedge-mapper` will have to clean the command topic with an empty retained message: 

```sh te2mqtt
tedge mqtt pub -r 'te/device/main///cmd/software_update/c8y-mapper-123' ''
```

