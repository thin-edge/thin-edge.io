---
title: 🚧 User-defined Operation Workflows
tags: [Reference, MQTT, Operations]
sidebar_position: 7
---

import ProposalBanner from '@site/src/components/ProposalBanner'

<ProposalBanner/>

## Overview

An agent developer can define application specific [operation workflows](./device-management-api.md#mqtt-driven-workflows)
to control how an operation is performed on a device.
Thin-edge **tedge-agent** provides the tools to:

- override existing workflows
- define new states and actions such as pre-requisite or post-execution checks 
- introduce new transitions such as rollbacks or conditional executions
- create new workflows, combining workflows and steps

## Example

Here is an example where three software components participate in a `firmware_update` command.
- The **tedge-mapper** creates the initial state of the command
  providing the required information to install a new version for a configuration file;
  and then waits for the final outcome (in black).
- The **tedge-agent** handles the main steps (in red): downloading the file and installing it where expected.
- User-provided scripts handle domain-specific checks (in blue)
  to timely schedule the command as well as to ensure the configuration file is not corrupted and properly installed.

```mermaid
stateDiagram-v2
    [*] --> init
    init --> download
    download --> downloaded
    downloaded --> install
    install --> installed 
    installed --> successful
    init --> failed
    download --> failed
    downloaded --> failed
    install --> failed
    installed --> failed
    successful --> [*]
    failed --> [*]
    
    classDef specific color:blue;
    class init, downloaded, installed specific
    
    classDef plugin color:red;
    class download, install plugin
       
    classDef mapper color:black;
    class successful, failed mapper
```

### Key points

Observe on the example that:

- At any state, *one and only one* participant is responsible to move the operation forward.
- Publishing a state to the MQTT command topic, can be seen as passing the baton from one participant to another.
  The mapper creates the **init** state and then lets the other components work.
  The agent tells the download has been successful by publishing the **downloaded** state,
  but do nothing till the domain-specific component has checked the file and move the command state to **install**.
- Each software component has to know only *some* states of the whole workflow:
  - the states they are responsible for
  - the states they create to pass the control.
- The compatibility of two participants, one advancing to the new state owned by the other, is only defined by the message payload:
  - all the property fields, required to make progress at some state, must be provided by the previous participant.

### Benefits

The benefits are that:
- A participant can be substituted by another implementation as long as the substitute implementation
  is ready to process at least all the states processed by the former implementation.
  - This is the key principle used by thin-edge to provide extensible operation support.
  - The **tedge-agent** defines the **downloaded** and *installed** states
    with no specific behavior beyond proceeding to the next step;
    so, a domain specific component can be substituted to add extra checks and actions before moving forward.
- Extra states and participants can be added as long as each state is owned by one participant.
  - For instance, an agent developer can introduce a **rollback** state in the `firmware_update` workflow,
    associated by another software component responsible for these rollbacks.

Furthermore, specific versions of the same workflow can be defined on different targets.
The main and child devices can each run their own version of a workflow for an operation.
Indeed, all the status updates for a command on a given thin-edge entity or component
are published on an MQTT topic with the entity identifier as the prefix.
- The same executable can be used to handle operations on different targets.
  For instance, the **tedge-agent** can run on the main device `te/device/main//`
  as well as on a child-device identified by `te/device/child-xyz//`.
- A specific executable can be substituted on a specific target.
  If for some reasons, **tedge-agent** cannot be installed on a child-device,
  then a specific implementation of the `firmware_update` MQTT API can be used to serve firmware updates
  on, say, `te/micro-controller/xyz//`.
- A workflow can be extended differently for each target.
  As an example, an agent developer can define an extra rollback state on the main device but not on the child devices.

## Operation API

As several software components have to collaborate when executing a command, each operation must define a specific API.
This API should be based on the principles of MQTT-driven workflow and defines:
- the well-known operation name such `firmware_update` or `restart`
- user documentation of the required input and the expected outcome of an operation request
- the set of observable states for a command and the possible state sequences
- for each state:
  - the well-known name such as **download** or **downloaded**
  - the schema of the state payload and the required parameters to process a command at this stage
  - developer documentation on the role of each parameter and the expected checks and actions
- the schema for the capability message sent when the operation is enabled on some thin-edge entity or component
  - developer documentation on the role of each field of the capability message

A workflow implementation is free to define the states a command can go through
as well as the message payload attached to each state.

However, there are some rules and best practices.

- Three states are mandatory: **init**, **successful** and **failed**.
- **init** must be the unique initial state of the workflow.
  - This state must give all the information required to execute the command.
  - Having no other initial states is important so any command requester (as the mapper)
    can trigger commands in a systematic manner *even* if the workflow is updated.
- **successful** and **failed** must be the unique terminal states of the workflow.
  - The payload of the **failed** state should contain a `reason` property telling what failed.
  - Having no other terminal states is important so any command requester (as the mapper)
    can await the termination of a command in a systematic manner *even* if the workflow is updated.
- A workflow API should define *no-op* states with no pre-defined actions
  and which sole purpose is to give an agent developer the opportunity to *insert* its own logic and extra steps.
  - A *no-op* state is a state which has a single direct transition to the next state.
  - As an example, having a **downloaded** *no-op* state, with a direct transition to an **install** state which uses the downloaded file,
    lets an agent operator override the automatic transition, say to check the downloaded file *before* moving the **install** state.
  - The **init** state should be a *no-op* state.

### Operation Implementation

A workflow implementation for a specific operation must implement the actions specified for each non *no-op* state.
This implementation has some freedom, notably to implement extra checks and actions but also to add new states.

However, there are some rules and best practices.

- All the state messages must be published as retained with QoS 1.
- A workflow implementation should not reject a state message payload with unknown fields.
  - It's also important to keep these unknown fields in the following states.
  - This is important as we want to *extend* the workflow of an operation.
    A software component added by the user might need these *extra* fields the plugin is not aware of.
- A workflow implementation must not react on *no-op* states nor terminal states.
  - The transition from a *no-op* state must be handled either by thin-edge as direct transition
    or overridden by the user with domain-specific checks and actions.
  - The terminal states, a.k.a **successful** and **failed**, are owned by the process which created the **init** state (in practice, the mapper).
    Only this process should clear the retained message state for an operation instance by sending an empty payload on command's topic.

### Workflow Overriding

Thin-edge provides a mechanism to override, extend and combine workflows.

This mechanism is provided by the **tedge-agent** which gathers a set of user-defined workflows
and combined them with the builtin workflows implemented by the agent itself.

Each workflow is defined using a TOML file stored in `/etc/tedge/operations`. Each specifies:
- the command name that it should trigger on
  such as `firmware_update` or `restart`- the list of states
- for each state:
  - the state name as defined by the operation API
  - the set of states which can be an outcome for this state actions
  - possible extra instructions on how to process the command at this stage, e.g.
    - run a script

```toml title="file: firmware_update_example.toml"
operation = "firmware_update"

[init]
  script = "/usr/bin/firmware_handler.sh plan"
  next = ["scheduled", "failed"]

[scheduled]
  next = ["install"]

[install]
  script = "/usr/bin/firmware_handler.sh install ${.payload.url}"
  next = ["reboot", "failed"]

[reboot]
  next = "verify"

[verify]
  script = "/usr/bin/firmware_handler.sh verify"
  next = ["commit", "rollback"]

[commit]
  next = ["successful", "rollback_reboot"]

[rollback]
  script = "/usr/bin/firmware_handler.sh rollback"
  next = ["rollback_reboot"]

[rollback_reboot]
  next = ["failed"]

[successful]
  next = []

[failed]
  next = []
```

Thin-edge combines all these workflows to determine what has to be done
when a state message is published for a command on a topic matching the global topic filter for commands,
i.e. `te/+/+/+/+/cmd/+/+`.
- Each running instance of the __tedge_agent__ reacts only on commands targeting its own device.
- If a user-defined workflow has been defined for this operation, then this workflow is used to determine the required action.
- If no workflow has been defined by the user for this operation, then the built-in workflow is used.
- If there is no workflow or no defined action for the current state, then the __tedge_agent__ fails the command.

### Built-in Actions

For operations supported by the __tedge-agent__, the built-in behavior is the default.
In other words, if no alternative actions is specified by a user-provided workflow,
the commands are processed following the built-in behavior.

If alternative actions are only given on some states, then the built-in behavior is applied to all others.
This gives the ability to override parts of the built-in operation workflow.

The __successful__ and __failed__ states are also handled in a specific way.
Only the command issuer is supposed to react on those,
pushing a retained empty message on the command request topic when done.
This ends the command workflow.

### Script Execution

A script can be attached to a command state. 

```
[state]
script = "/full/path/command [args]" 
```

This script is given as a plain command line possibly with arguments.

Data extracted from the command status topic and message payload can be passed as argument to the script.

- `"/bin/new-command.sh ${.topic} ${.payload}"` passes two arguments to the `/bin/new-command.sh` program.
  - The first one is the full command request topic (*e.g.* `te/device/main/cmd///restart/c8y-mapper-123`).
  - The second one is the full json payload (*e.g.* `{"status': "init"}`).
- Specific path expressions can be used to pass specific excerpts.
  - `${.}` is a json for the whole message including the `topic` and the `payload`.
  - `${.topic}` is the command request topic (*e.g.* `te/device/main/cmd///restart/c8y-mapper-123`)
  - `${.topic.target}` is the command target identity  (*e.g.* `device/main/cmd//`)
  - `${.topic.operation}` is the command operation  (*e.g.* `restart`)
  - `${.topic.cmd_id}` is the command request unique identifier  (*e.g.* `c8y-mapper-123`)
  - `${.payload}` is the whole command json payload (*e.g.* `{"status': "init"}`)
  - `${.payload.status}` is the command current status (*e.g.* `"init"`)
  - `${.payload.x.y.z}` is the json value extracted from the payload following the given `x.y.z` path if any.
  - If given `${.some.unknown.path}`, the argument is passed unchanged to the script.

The script exit status and output is used to determine the next step for the command.
- If the script cannot be launched or return a non-zero status, the command request is marked as __failed__.
- If the script successfully returns, its standard output is used to update the command state payload.
  - From this output, only the excerpt between a `:::begin-tedge:::` header and a `:::end-tedge:::` trailer is decoded.
    This is done to ease script authoring. A script can emit arbitrary output on its stdout,
    and just have to surround its workflow updates with the `:::begin-tedge:::` and `:::end-tedge:::` markers.
  - If this excerpt is a json payload, this payload is injected into the previous message payload
    (adding new fields, overriding overlapping ones, keeping previous unchanged ones).
  - If this excerpt is a json payload with a `status` field, then this status is used as the new status for the command.
- If the script output is empty, then the exit status of the process is used to determine the next step. 

### Restart action

A workflow can trigger a device restart, using the builtin __restart__ action.

This action is controlled by three states:
- the *on_executing* state to which the workflow moves before the reboot is triggered
- the *on_success* state to which the workflow resumes after a successful device reboot
- the *on_error* state to which the workflow resumes in case the reboot fails

For instance, the following triggers a reboot from the `reboot_required` state
and moves to `restarting` waiting for the device to restart
and finally to either `successful_restart` or `failed_restart`,
depending on the actual status of the reboot. 

```
[reboot_required]
script = "restart"
next = ["restarting", "successful_restart", "failed_restart"]
```

:::note
This file format is not finalized and will likely be revised.
:::