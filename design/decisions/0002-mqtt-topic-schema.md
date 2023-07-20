# MQTT Topic Design Decision

* Date: __2023-07-14__
* Status: __Approved__
* Outcome: Proposal 3 was selected for implementation

**Note**

* During implementation, if small adjustments are required, the corresponding reference and design pages will be update, however this decision page will not.

# Background

The inconsistency in the existing topic schemes of thin-edge has long been a problem for both users and the
thin-edge dev team to write new applications or new extensions of thin-edge.

Here are a few such examples:
1. Topic for events: `tedge/events/{event-type}/{child-id}`
1. Topic for firmware update commands: `tedge/{child-id}/commands/req/firmware_update`
1. Topic for software update commands: `tedge/commands/req/software/list`

Where there is inconsistency in the placement of child devices and how the commands are grouped.

There are a few other limitations like the lack of support for services on the thin-edge device,
difficulty in extending existing topics with additional sub-topics, etc
which are detailed in the requirements section.

# Domain model influence

The MQTT topics and interactions are modelled around the following entities:

1. **Device**
   A device can be viewed as a composition of hardware components and firmware/software components running on it.
   The device can extract data from these hardware components and emit that as telemetry data using some software on it.
   The device can also control these hardware components using some software.
   The device also manages the firmware and all the software running on it.
   A device could be connected to many other devices as well as multiple cloud providers/instances.
1. **Tedge device**
   The gateway device that is connected to the cloud, where thin-edge.io is installed,
   which emits its own telemetry data and can receive commands to perform operations on the device.
2. **Child device**
   Typically, a different device that is connected to the tedge device, which has its own identity
   which is separate from the tedge device itself.
   It also emits its own measurements and can receive commands.
   A child device need not always be a a physical device, but can also be a logical one,
   abstracting some parts or groups of parts of the main device as well, with its own identity.
   A child device can have further nested child devices as well.
3. **Service**
   A service can be a component or a piece of software running on a device (tedge device or child device).
   For example, a device can have a cloud connector/agent software that can be viewed as a service.
   Any software on the device can be modelled as a service, if monitoring them separately from the device makes sense.
   This abstraction be also be used to isolate various aspects of the device itself into separate groups, but still linked to the device.

   A service can have its own telemetry data which is separate from the device's telemetry data.
   For e.g: a service running on a device can report its own RAM or disk usage,
   which is separate from the device's overall RAM or disk usage.
   The services are also usually managed (installed, updated etc) by the device that it is running on and hence
   all commands meant for services are received and managed by that device.
   For e.g: It would be much easier for the device to update/uninstall a service than expecting the service to update itself.
   But, thin-edge does not completely rule out the possibility of services handling commands on their own as well, in future.
   When a service is removed from a device, all data associated with it are obsolete as well, and hence removed.

   Unlike devices that only has a connectivity status, services have a liveness aspect to them as well,
   which conveys if a service is up and running at any given point of time.
   The liveness of services could be critical to the functioning of that device.

   A service does not support nested services either.

When a thin-edge device receives telemetry data for itself or child devices or services running on any of them,
it needs to identify their source so that they can be associated with their twins on the device as well as in the cloud.
For all the MQTT communication, this source info can be part of either the topic or the payload.
Having them in the topics is desired, as that enables easy filtering of messages for a given device or a subset of devices.

# Example deployment

Here is a sample deployment structure for a smart two-wheeler that is used in the examples below:

```mermaid
graph TD;
    TEDGE --> CENTRAL-ECU-0001;
    CENTRAL-ECU-0001 --> ENGINE-ECU-E001;
    CENTRAL-ECU-0001 --> WHEEL-ECU-E001;
    CENTRAL-ECU-0001 --> WHEEL-ECU-E002;

    ENGINE-ECU-E001 --> TEMP-1001*((TEMP-1001));

    WHEEL-ECU-E001 --> BRAKE-ECU-B001;
    WHEEL-ECU-E001 --> TYRE-ECU-T001;
    BRAKE-ECU-B001 --> TEMP-1001((TEMP-1001));
    TYRE-ECU-T001 --> TEMP-1002((TEMP-1002));
    TYRE-ECU-T001 --> PRSR-1002((PRSR-1002));

    WHEEL-ECU-E002 --> BRAKE-ECU-B001'[BRAKE-ECU-B001];
    WHEEL-ECU-E002 --> TYRE-ECU-T002;
    BRAKE-ECU-B001' --> TEMP-1001'((TEMP-1001));
    TYRE-ECU-T002 --> TEMP-1003((TEMP-1003));
    TYRE-ECU-T002 --> PRSR-1003((PRSR-1003));

    style BRAKE-ECU-B001 fill:#0f0
    style BRAKE-ECU-B001' fill:#0f0

    style TEMP-1001* fill:#fa0
    style TEMP-1001 fill:#fa0
    style TEMP-1001' fill:#fa0
```

As you can see, the ECUs for the front and rear wheels have unique IDs: `WHEEL-ECU-E001` and `WHEEL-ECU-E002`,
as they exist at the same level, connected to the central ECU(`CENTRAL-ECU-0001`).
But, the brake ECUs connected to both the wheels could have the same ID, as they are not linked directly anyway.
Even the sensors attached at many levels in such a complex deployment may have the same IDs(`TEMP-1001`).

So, the proposed solutions must address such ID clashes in a deep nested hierarchies.

:::info
Even the parent-child `id` combination for all devices is not unique in this deployment,
as you can see that the temperature sensors on the brakes of both front wheel and rear wheel
have a parent-child ID combination of `BRAKE-ECU-B001/TEMP-0001`
:::

# Use-cases

1. Support for nested child devices.
   A deployment where a gateway device is connected to a PLC which is further connected to other sensors/devices is very common.
   There are 3 levels of devices even in this simple deployment which a user might want to replicate in his cloud view as well.
   More complex deployments where a PLC is further connected to more PLCs which are further connected to leaf sensors/actuators
   would require even more levels of nesting.
   When a child device has its own nested child devices, it is expected that the parent child device sends/receives data
   on behalf of all its direct children.
   So, there must be a way for a child device to subscribe for all data meant for itself and its own child devices,
   excluding its siblings and their child devices.
1. Monitor the liveness of a piece of software running on a device (tedge or child) from the cloud.
   Certain services running on a device could be critical to the overall functioning of that device.
   Hence monitoring the health/liveness of these services also would be critical.
1. Gather the usage metrics of a software component(service) running on a device as measurements pushed to the cloud,
   associated to an entity representing that software linked to that device, and not the device itself.
   An identity separate from the device is key here, to ease the management of that component from the cloud.
   It is also required so that when that component is removed from the device, all data associated with it is removed as well.
   It must be linked to a device as software component does not have independent existence and is managed by a device.
   When a device is removed, all services linked to it are removed as well, as they're obsolete without the device.
1. When data from services, connected child devices and even the tedge device itself, are flowing through the MQTT broker,
   it must be easy to identify and filter the messages based on the source.
   A few examples of filtering queries are:
   * All measurements from a specific service
   * All measurements from the tedge device only, excluding the ones from other services and child devices
   * All measurements from all connected child devices
   * All measurements from everything (the tedge device itself, its services and child devices and their services)
   * All messages from a subset of child devices based on some filtering criteria (e.g: device-type, firmware version etc)
1. Since services are typically software components, and the same component would be running on multiple devices,
   the names/ids of these components could be the same on all devices.
   We can't expect the service names to be unique across a large fleet of devices either.
   Although we can force the service developers to keep them unique with UUID **suffixes**
   (e.g: `tedge-agent-abcaa88d-8e4f-4272-84fc-fead2a8890b0`) or something like that,
   it would be better to avoid this, as they are really not very user friendly.
   Hence, service ids must be namespaced under each device that it is running on.
1. Child device ids also must be namespaced under their direct parent
   so that conflicts can be avoided even if another parent device has a child device with the same id.
   It is okay to expect all devices connected to a device to have unique ids,
   but expecting those to be unique across an entire fleet could be far-fetched.
1. If all child devices in a fleet can not guarantee uniqueness in their IDs,
   thin-edge must provide a "registration mechanism" for them to get their own unique IDs,
   which they can use for all their HTTP/MQTT communications.
1. Device registration must be optional if a fleet admin can guarantee unique IDs for all his devices.
   When an explicit registration is optional, child devices get auto-registered on receipt of the the very first message from them.
1. When multiple child devices are connected to a tedge device,
   a given child device should only be able to send/receive data meant for itself and not a sibling child device.
   Thin-edge must provide this isolation in such a way that the even peer child devices can not even view others' data.
   But, when a child device has its own nested child devices, it is expected that
   the parent child device sends/receives data on behalf of all its children.
1. Connect to multiple cloud instances, even of the same provider, at the same time.
   This is a common deployment model for SEMs, where the devices that they produce are labelled and sold by solution providers to end customers.
   The device will be connected to the solution provider's cloud to gather all business data from the customer.
   But, it might have to be connected simultaneously to the SEM's cloud as well so that the SEM can remotely manage that device (for periodic maintenance).
1. All the existing topics like `tedge/measurements`, `tedge/events` imply that the data received on these
   must be forwarded to the cloud as well.
   Currently there is no way to tell thin-edge to just route some data internally and not forward those to the cloud.
   Since filtering and aggregation on the edge is a very common use-case, especially for local analytics, this is highly desired.
1. Enable thin-edge extensions/plugins to register themselves with thin-edge by declaring their capabilities(supported commands).
   Child devices also should be able to do the same so that thin-edge is aware of the supported capabilities of each child device,
   for routing and management of commands meant for them.
1. Keeping the entire device hierarchy in each and every message sent by a device must be avoided,
   as that data is highly redundant and could impact the message sizes badly (sometimes bigger than the payload itself).
   When a device is sending its telemetry data, it shouldn't have to repeat its lineage each and every time.
1. When child devices have their own MQTT brokers on them, it is desired that all the messages received by that broker
   can be routed to the tedge MQTT broker using some static routing rules defined on that child broker.
   Especially, if tedge is running on those child devices as well, this must be feasible.
   You can even assume a nested chain of MQTT brokers in a hierarchical deployment,
   where each broker routes all traffic to its parent, which is further routed by that parent to its parent.
1. **TODO**: Use-case for swapping child devices in a live deployment

# Assumptions

1. All services under a given device will have unique ids, namespaced under that device id.
   But the same service name might repeat under multiple devices.
1. All child devices under a given device will have unique ids, namespaced under that device id.
   But different child devices under different parents might have the same name.
1. When device IDs are used in topics for child devices, it is expected that all of them have unique IDs.
   They can use the thin-edge registration service to get unique IDs assigned, if they don't have it on their own.
   These IDs need not be globally unique or even unique across multiple thin-edge devices.
   They just need to be unique under the tedge network that they are part of.

# Requirements

This section is divided into 3 parts:
1. Must-have: for the requirements that must be met by the proposed solutions
2. Nice-to-have: these requirements are not mandatory, but the solution addressing more of these requirements would be a plus
3. Out-of-scope: those that are relevant but out of scope for this design exercise

## Must-have

1. All MQTT topics for main device, child device and services must start with a common prefix like `tedge/`,
   so that all tedge traffic can be filtered out easily from all other data being exchanged over the MQTT broker.
1. The MQTT messages must capture the source of that data that is exchanged through that message:
   whether it came from the tedge device, a child device or a service on any one of them.
   Keeping them in the topics would make source-based filtering of messages easier.
1. Support nested child devices so that telemetry data and commands can be exchanged with child devices of child devices.
1. A separate topic structure to exchange data with services running on a device (main or child)
   which is different from the topics of the device itself.
1. Service `id`s must be namespaced under the device that it is linked to.
   The ids of different services on the same device must be unique,
   but services with the same name/id would be running on multiple devices.
1. A registration service provided by thin-edge to provide unique IDs for requesting child devices.
   This registration step must be optional for devices if they can guarantee unique IDs on their own.
1. Support the following kinds of filtering with minimal effort (ideally with a single wildcard subscription):
   * All data from thin-edge device excluding everything else(other services and child devices)
   * All data from all child devices excluding those from thin-edge device and services
   * All data from all services excluding those from thin-edge device and child devices
   * All data from a specific child device excluding everything else
   * All data from a specific service excluding everything else
1. The topic structure should be ACL friendly so that rules can be applied
   to limit devices and services to access only the data meant for them.
1. Enable easier extension of topics with further topic suffixes in future:
   E.g: Support `type` in the topic for measurements like `tedge/measurements/{measurement-type}`
   or config-type in the topic for config management command: `tedge/commands/req/config_update/<config-type>`
1. Enable exchange of telemetry data just on the local bus, without it getting forwarded to the cloud.
1. Thin-edge must be able to generate unique `id`s for child devices on the local network,
   without consulting with the cloud.
   The device registration service must work even if the tedge device is not connected to a cloud.
   All translation between tedge-local-ids and cloud-twin-ids should be done internally by the cloud mapper.
   It is okay for the mapper to use the same tedge-local-ids in the cloud, if they are unique enough.
   But, the cloud IDs (typically very long and cumbersome) must not be exposed to the tedge network at any cost.
1. A registration mechanism for thin-edge plugins to declare their capabilities and other necessary metadata to thin-edge.
1. Enable a device to subscribe to all the data meant for itself, its services and all its descendent child devices.
   This subscription mechanism should prevent a device from subscribing to messages of all other devices.
1. Avoid entire device hierarchy in message payloads to avoid sending redundant data every time some data is sent.
   The parent hierarchy of a child device must be established only once, during its registration
   and thin-edge must be able to map the parent hierarchy from its unique id from then on.
1. Easy to create static routing rules so that it is easy to map a local MQTT topic for a nested child device

## Nice-to have

1. Avoid using the device id in the topics for the main tedge device to keep those well-known topics simple.
   Use aliases like `main` or `root` instead, if an identifier is really required.
   Using `id`s for child devices might be unavoidable.
1. Consistency/symmetry in the topic structures for the main device, child device and services.
   For e.g: if the telemetry topic for a child device is like: `tedge/level2/level3/level4/telemetry/...`
   it would be better if the `telemetry` subtopic is at the 5th level in the topics for the main device and services as well,
   as that would make wildcard subscriptions like "all telemetry data from all sources" easier.
1. Dynamic creation/registration of child devices on receipt of the very first data that they send.
   This is desired at least for immediate child devices, if not for further nested child devices.
   to the equivalent cloud topic for its twin.
1. It would be ideal if the context/source of data (tedge device, service or child device)
   can be understood from the topic directly.
   For e.g: a topic scheme like `tedge/main/{id}`, `tedge/service/{id}` and `tedge/child/{id}` is more user-friendly
   than a simpler context agnostic scheme like `tedge/{id}` where `id` can be for any "thing".
1. Limit the topic levels to 7 as AWS IoT core has a [max limit of 7](https://docs.aws.amazon.com/whitepapers/latest/designing-mqtt-topics-aws-iot-core/mqtt-design-best-practices.html)

## Out of scope

1. Routing different kinds of data to different clouds, e.g: all telemetry to azure and all commands from/to Cumulocity.
   Even though this requirement is realistic, thin-edge MQTT topics must not be corrupted with cloud specific semantics,
   and the same requirement will be handled with some external routing mechanism(e.g: routing via bridge topics)

# Proposals

## Proposal 1: Dedicated topics for devices and services

This proposal is built on the following assumptions/constraints:
* thin-edge can identify the child devices (the nested ones as well), and all the services with unique IDs.
* All child-devices connected to a parent device will have unique IDs under its parent namespace,
  but not necessarily across the entire tedge namespace/hierarchy.
* If all child devices **can not** guarantee that uniqueness across the entire hierarchy,
  then they are expected to register with thin-edge to get that unique ID generated.
* Given that unique ID, thin-edge can identify its entire parent lineage and descendent child devices.
  Thin-edge internally maintains this mapping information as child devices register with it.
* If all child devices **can** guarantee ID uniqueness across the entire hierarchy,
  they can skip this registration step.
* Services need not have unique IDs across the entire hierarchy.
  They just need to have unique IDs under their parent's namespace.

The proposal is as follows:

Topics to have device id as the top level prefix with distinction on the target: device or service as in:

```
tedge/<device-id>/<target-type>/...
```

The following demonstrates the different topic prefix for each of the target types:

|Target|Topic Prefix|
|------|-------|
|tedge device|`tedge/main/device/`|
|service on tedge device|`tedge/main/service/<service-id>/`|
|child device|`tedge/<child-id>/device/`|
|service on child device|`tedge/<child-id>/service/<service-id>/`|

Where `main` is used as an alias for the tedge device id.

**Why have the `/device/` subtopic level?**

To make a clear distinction between `device` data and `service` data.
This distinction makes it easier to subscribe to all device only data excluding the service data
with simple subscription filters like `tedge/main/device/#`.
Without that device suffix, something like `tedge/main/#` would include the services' data as well.


**Why have the /service/ subtopic level and not have the service-id directly?**

Primarily for future-proofing as `service` is a new kind of child abstraction that we've added now.
If another abstraction is introduced in the future, say `plugin`, they can be namespaced under a `plugin/` subtopic.
It also makes the distinction from `device` data clearer.

### Telemetry

For telemetry data, the topics would be grouped under a `telemetry/` sub-topic as follows:

|Type|Example|
|------|-------|
|Measurements|`<target_prefix>/telemetry/measurements/[/<measurement-type>]`|
|Events|`<target_prefix>/telemetry/events/<event-type>`|
|Alarms|`<target_prefix>/telemetry/alarms/<alarm-type>`|

The `measurement-type` is optional for `measurements`.
The alarm `severity` which was earlier part the topic have been moved to the payload.

**Examples**

The following shows an example of the measurement telemetry topics for each of the target types:

|Target|Topic structure|
|------|-------|
|tedge device|`tedge/main/device/telemetry/measurements`|
|service on tedge device|`tedge/main/service/<service-id>/telemetry/measurements`|
|child device|`tedge/<child-id>/device/telemetry/measurements`|
|service on child device|`tedge/<child-id>/service/<service-id>/telemetry/measurements`|

**Why have the redundant `/telemetry` subtopic level?**

To have a clear separation from commands or other kinds of data that might get added in future.
It simplifies subscriptions for "all telemetry data from a device" to just `tedge/<device-id>/+/telemetry/#`.
Without that `telemetry` grouping level, doing such a subscription would be difficult as you'll have to subscribe to each of the following topics separately:

* `tedge/<device-id>/+/measurements/#`
* `tedge/<device-id>/+/events/#`
* `tedge/<device-id>/+/alarms/#`

In additional, this topic structure allow to combine the subscription of telemetry data as well as command data via the following MQTT wildcard topic:

```
tedge/<device-id>/+/#
```

Creating ACL rules is also simplified by allowing telemetry data to be controlled independently to the command topics.

### Commands

Similarly, all commands would be grouped under a `commands/` sub-topic as follows:

|Target|Topic structure|
|------|-------|
|For requests|`tedge/<device-id>/<target-type>/commands/req/<operation-type>/<operation-specific-keys>...`|
|For responses|`tedge/<device-id>/<target-type>/commands/res/<operation-type>/<operation-specific-keys>...`|

The `operation-specific-keys` are optional and the number of such keys (topic levels) could vary from one operation to another.

**Examples:**

* Software list operation on main device

   ```
   tedge/main/device/commands/req/software_list
   tedge/main/device/commands/res/software_list
   ```

* Software update operation on child device

   ```
   tedge/<child-id>/device/commands/req/software_update
   tedge/<child-id>/device/commands/res/software_update
   ```

* Firmware update operation

   ```
   tedge/main/device/commands/req/firmware_update
   tedge/main/device/commands/res/firmware_update
   ```

* Device restart operation

   ```
   tedge/main/device/commands/req/device_restart
   tedge/main/device/commands/res/device_restart
   ```

* Configuration snapshot operation on a main device service

   ```
   tedge/main/service/<service-id>/commands/req/config_snapshot
   tedge/main/service/<service-id>/commands/res/config_snapshot
   ```

* Configuration update operation on a child device service

   ```
   tedge/<child-id>/service/<service-id>/commands/req/config_update
   tedge/<child-id>/service/<service-id>/commands/res/config_update
   ```

Although all the above examples maintain consistent structure by ending with the `<operation-type>`,
further additions are possible in future if desired for a given operation type.
For e.g: `tedge/main/commands/req/config_update/<config-type>` to address a specific `config-type`

### Registration service for child devices

Immediate and nested child devices can be registered with thin-edge using its registration service,
by sending the following MQTT message to the topic: `tedge/main/register/req/child`:

```json5
{ 
   "parent": "<parent-device-id>",
   "id-prefix": "<desired-child-id-prefix>",
   "capabilities": {
      "<capability-1>": {},  //capability-1 specific metadata
      "<capability-2>": {},  //capability-2 specific metadata
      // ...
   }
}
```

The `parent-device-id` is the device-id of the direct parent that the child device is connected to.
The payload can have other fields describing the capabilities of that device as well (config management, software management etc).

:::caution
The exact topic keys and payload format for this init contract can be discussed and refined separately.
The focus here is just on the MQTT topic structure.
:::

Thin-edge needs to maintain the lineage (hierarchy of parent devices) of all descendent child devices in its internal state,
so that it can be looked up while receiving any data from them.
Even though the child device only declares its immediate parent in the registration message,
the entire lineage can be traced back with a recursive lookup on that `parent-device-id`.

The registration status, whether the device registration succeeded or not, is sent back on `tedge/main/register/res/child`,
with the internal device id used by thin-edge to uniquely identify that device as follows:

```json
{
   "id": "<generated-id>",
   "status": "successful" 
}
```

The `<generated-id>` will use the `<desired-child-id-prefix>` sent in the registration request.

A failure is indicated with a failed status: `{ "status": "failed" }`.

Once the registration is complete, these nested child devices should use the `tedge/<generated-id>` topic prefix
to send telemetry data or receive commands as follows:


|Type|Topic structure|
|----|---------------|
|Measurement (device)|`tedge/<internal-id>/device/telemetry/measurements`|
|Command (device)|`tedge/<internal-id>/device/commands/req/software_list`|
|Measurement (service)|`tedge/<internal-id>/service/<service-id>/telemetry/measurements`|
|Command (service)|`tedge/<internal-id>/service/<service-id>/commands/req/config_snapshot`|

### Automatic registration

Automatic registration is supported for services as they are expected to have unique names under each device namespace,
and they do not supporting nesting, eliminating any name clashes that way. 
So, it is easier to associate those services directly with their parent devices, as the parent id is part of the topic.

It can't be supported for child devices with this topic scheme as there's no distinction between
immediate child devices and nested child devices in the topics.

If it really needs to be supported, at least for immediate child devices,
they need to declare somehow that they are immediate child devices or not,
which can be done by adding that distinction in the topics as follows:

|Child type|Topic structure|
|----|---------------|
|(immediate) child device|`tedge/child/<child-id>/...`|
|nested child device|`tedge/descendent/<child-id>/...`|


.. so that automatic registration can be done for everything coming from `tedge/child/...` topics.
If keeping that information in the topics is not desired, it can be kept in the payload as well,
with the caveat that defining static routing rules would not be possible with it.

**Pros**

1. The context on whether the data is coming from the parent, a child or a service is clear from the topics.
1. Automatic registration is possible at least for services and even child devices with some tweaks.

**Cons**

1. The topics are fairly long and with extensions might easily cross the 7 sub-topic limit of AWS.

## Extended Proposal 1: Include parent id also in the topics

Include the parent `id` information also in the topic as follows:

```
tedge/<parent-device-id>/<device-id>/<target-type>/...
```

Examples:

|Target|Topic Prefix|
|------|-------|
|tedge device|`tedge/main/device/`|
|service on tedge device|`tedge/main/service/<service-id>`|
|immediate child device|`tedge/main/<child-id>/device/`|
|descendent child device|`tedge/<parent-child-id>/<child-id>/`|
|service on child device|`tedge/<child-id>/service/<service-id>/`|
|service on descendent child device|`tedge/<parent-child-id>/<child-id>/service/<service-id>/`|

For the main device, there is no change as it is the root device,
unless we want to make it more explicit and symmetric with a `root` prefix like `tedge/root/main/device/`

**Pros**

* When the parent device wants to subscribe to all data from its immediate child devices,
  it can be done with a `tedge/<parent-child-id>/+/#` subscription.
  This single subscription will work even when child devices are added dynamically.
  Otherwise, the parent will have to subscribe for all child devices individually
  and dynamically subscribe to new child topics, when new child devices are added.

**Cons**

* Topics are even longer for marginal gains.
* Higher payload size, as the parent name is repeated in each and every message.
* To subscribe for all nested child devices data as well, the parent will have to do dynamic subscriptions anyway
  when a new nested child device is added.
  For example, if a `parent` is already subscribed to `tedge/parent/+/#` for all child devices: `child-1` and `child-2`,
  when a `child-3` is added, it will be covered with the existing subscription itself.
  But, if these child devices have nested child devices, the parent will have to subscribe to,
  `tedge/child-1/+/#` and `tedge/child-2/+/#` separately,
  and when `child-3` is added, dynamically subscribe to `tedge/child-3/+/#` as well.

## Proposal 2: Unified topics for every "thing"

This proposal is built on top of the following assumptions:
1. Every entity, a child device, descendent child devices or services, must register with thin-edge
   to get their unique IDs generated, unless these entities can ensure uniqueness across the entire fleet.
1. No dynamic registration for anything.
1. Avoiding the entity type(device or service, child or descendent etc) from the topics reduces the payload size as well,
   as the type of an entity needs to be declared only once and need not be repeated over and over for every message.

The proposal is as follows:

A topic scheme that doesn't differentiate between parent child devices or services and
does not declare the parent-child relation in the topics either, as follows:

```
tedge/<entity-id>/...
```

...where `id` could be the `id` of the main device, child device or a service on any of them.
The alias `main` can still be used for the main tedge device.

Examples:
* Main device measurements: `tedge/<tedge-device-id>/telemetry/measurements`
* Main device service measurements: `tedge/<service-id>/telemetry/measurements`
* Child device measurements: `tedge/<child-device-id>/telemetry/measurements`
* Child device service measurements: `tedge/<child-service-id>/telemetry/measurements`

:::info
`tedge/main` could be just used as an alias for `tedge/<tedge-device-id>` for simplicity.
:::

**Pros**

1. Due to the lack of distinction in the topics between parent devices and child devices,
   it is easier to write code for "the device" irrespective of whether it is deployed as a parent or child.
1. Minimal payload size as we're only including the `id`s everywhere without repeating any context info.

**Cons**

1. Not easy to differentiate the context(from parent or child) easily from the message.
1. The `id`s must be unique between all devices and services in a deployment.
1. Not easy to do subscriptions like: "measurements only from child devices or only from services, excluding parent",
   without keeping track of all the `id`s of services registered with that child device.
   Easy wild card subscriptions are not possible at all.
1. No automatic registration as bootstrapping is mandatory for everything.

## Proposal 3: Namespace extensible topics

This scheme splits the topic levels into two parts:
* entity/component identification levels
* data type identification levels

This scheme reserves 4 subtopic levels for entity identification,
and 2 levels for data type identification such as measurements, alarms, events, commands etc.

One way to model the topics would be as follows:

```
te/<entity-namespace>/<entity-id>/<component-namespace>/<component-id>/<data-type>/<data-instance-type>
```

Where 2 subtopics are used to identify top level entities like devices,
and the 2 further subtopics are used to identify components on those entities like services on devices.
But, thin-edge doesn't enforce/dictate this scheme.
The users are free to model the topic levels as per their requirements.
Here are a few alternative ways to model the topic levels:

* `te/<device-namespace>/<device-id>/<service-namespace>/<service-id>/...`
* `te/<device-namespace>/<device-id>/<device-local-namespace>/<device-local-component-id>/...`
* `te/<device-family>/<device-series>/<part-series>/<part-id>/...`
* `te/<parent-device>/<1st-level-child-device>/<2nd-level-child-device>/<service-id>/...`

Having 4 levels for entity identification provides enough flexibility to model various use-cases
with separate namespaces for devices and applications running on them, minimizing the risk of `id` conflicts.

Even for the data type levels, a user is free to define those as they wish.
But thin-edge has some pre-defined `<data-type>` subtopics for well-known types like
*measurements*, *alarms*, *events* and *commands*, on which it enforces some constraints.

**Subtopics for predefined telemetry data**

|Type|Example|
|------|-------|
|Measurements|`<entity_id_prefix>/m/[/<measurement-type>]`|
|Events|`<entity_id_prefix>/e/<event-type>`|
|Alarms|`<entity_id_prefix>/a/<alarm-type>`|
|Data (Inventory)|`<entity_id_prefix>/data/<data-type>`|

**Subtopics for predefined commands**

|Type|Example|
|------|-------|
|Software List|`<entity_id_prefix>/cmd/software_list`|
|Software Update|`<entity_id_prefix>/cmd/software_update`|
|Configuration Snapshot|`<entity_id_prefix>/cmd/config_snapshot`|
|Configuration Update|`<entity_id_prefix>/cmd/config_update`|
|Firmware Update|`<entity_id_prefix>/cmd/firmware_update`|
|Restart[^1]|`<entity_id_prefix>/cmd/restart`|

[^1]: Restart would mean a device restart or a service restart based on the target entity

### Entity registration

Since thin-edge doesn't enforce what each device identification level means,
an explicit registration is required to register every entity that is going to send data or receive commands.
For example, before a measurement can be sent from a service named `tedge-agent` from the device `Rpi1001`,
the entity named `Rpi1001` must be registered as a `device`, and `tedge-agent` must be registered as a `service`.

An entity can be registered with thin-edge by publishing a retained message to the entity identification topic prefix
with the entity type and other metadata that defines that entity.
To model the example mentioned above, if an entity identification topic scheme like the following is used:

```
te/<device-namespace>/<device-id>/<service-namespace>/<service-id>
```

The `Rpi1001` device must be registered as a `device` by publishing the following retained message:

```sh te2mqtt
tedge mqtt pub -r te/_/Rpi1001 '{
  "type": "device"
}'
```

The device is registered under the default namespace indicated by `_` at the second level.
The registration message supports additional fields like `name`, `parent`, `device-type` etc as well,
which will be explained in detail later.

Once the device is registered, the `tedge-agent` service can be registered as a `service` type as follows:

```sh te2mqtt
tedge mqtt pub -r te/_/Rpi1001/_/tedge-agent '{
  "type": "service"
}'
```

Once these entities are registered, they can start sending data or receive commands.

Examples:

* Device measurement:

   ```sh te2mqtt
   tedge mqtt pub -r te/_/Rpi1001/_/_/m/battery_reading` '{
      "charge": 88
      "temperature": 32,
      "voltage": 45,
      "current": 15
   }'
   ```

* Service measurement

   ```sh te2mqtt
   tedge mqtt pub -r te/_/Rpi1001/_/tedge-agent/m/cpu_usage` '{
      "usage": 23
      "threads": 36,
      "up_time": 3726,
   }'
   ```

If the the user had used a different entity identification scheme like
`te/<device-family>/<device-series>/<part-series>/<part-id>/...` instead,
where the `<part-id>` is the unique device identifier,
the registration message should have to be sent to the `te/<device-family>/<device-series>/<part-series>/<part-id>` topic,
with the `type` as `device`.

So, it is completely up-to the user to choose how many levels he wants to uniquely identify devices and services.

If your use-case doesn't need so many levels to identify an entity,
the unnecessary levels can be skipped with the `_` (underscore) character as follows:

```
te/<entity-id>/_/_/_/...
```

or 

```
te/_/_/_/<entity-id>/...
```

**Main Device**

The main device does not need any explicit registration and can be referred to using the `main` alias as device id.
Hence, a measurement associated to the main device can be sent to following topic:

```
te/_/main/_/_/m/battery_reading
```

But if some additional metadata needs to be added to the main device,
they can be pushed to the `te/_/main` topic as done using a registration message.

**Immediate Child Devices**

Immediate child devices of the main device can be registered using the registration protocol described above.
If a device is registered with `main` as the `parent` field value, or without a `parent` field,
it is assumed to be an immediate child device.

**Nested Child Devices**

Nested child devices must always register explicitly by declaring their `parent`.

### Automatic registration

If an explicit registration is not done, thin-edge will assume the following topic level convention:

```text
te/<device-namespace>/<device-id>/<service-namespace>/<service-id>
```

and auto-register the entities as per their positions in the topics.

For example, if the following measurement message is received without any explicit registrations,

```text
te/abc/Rpi1001/xyz/collectd/m/cpu_usage
```

`Rpi1001` and `collectd` will be auto-registered as `device` and `service` types,
with their `device type` and `service type` derived from their respective namespaces.

### Data type metadata

The data types also may have additional metadata associated with it,
which can be added/updated by publishing to `/meta` subtopics of those data types.
For example, the units associated with measurements in the `battery_reading` measurement type
can be updated by publishing the following message:

```sh te2mqtt
tedge mqtt pub -r te/_/Rpi1001/_/_/m/battery_reading/meta '{
  "units": {
    "temperature": "C",
    "voltage": "V",
    "current": "A"
  }
}'
```

The metadata fields supported by each data type will be defined in detail later.

### Backward compatibility

A compatibility layer should be implemented to map the existing (legacy) tedge topics to the new topic structure. It is important that the layer translate from the legacy to the new so that both existing and new entities/components are both observable via the new topics.

The compatibility layer should be able to be deactivated via configuration after all entities/components have been migrated.

Below details some examples showing the mapping between the legacy and new topics:

|Legacy topic|New topic|Notes|
|------------|---------|-----|
|`tedge/measurement`|`tedge/device/main///m/ThinEdgeMeasurement`|
|`tedge/events/<type>`|`tedge/device/main///e/<type>`|
|`tedge/alarms/<severity>/<type>`|`tedge/device/main///a/<type>`|Severity will be mapped to the `.severity` property in the payload|

## Comparison

Here is a comparison of both proposals against all the key requirements:

`"` means the same as the previous proposal.


| Requirements | Proposal 1: Context in topics | Extended proposal 1 | Proposal 2: ID only topics |
| --- | --- | --- | --- |
| **_Must-Haves_** |
| Support nested child devices | Yes | Yes | Yes |
| Support services | Yes | Yes | Yes |
| Dynamic registration for child devices | Yes | Yes | Yes |
| Dynamic registration for services | Yes | Yes | No |
| Dynamic registration for nested child devices | No | Yes | No |
| Limit data access to a given entity | Yes | Yes | Yes |
| Supports extension for new entities | Yes | Yes | Yes|
| Supports extension for new data specific types | Yes | Yes | Yes|
| Support local-only data exchange | Introduce new topic level under `tedge/` | " [^1] | Use a namespace other than `tedge/` |
| Message size | High | Highest | Least |
| --- | --- | --- | --- |
| _Filtering capabilities_ |
| * All data from everything | `tedge/#` | `tedge/#`  | `tedge/#` |
| * All thin-edge device data excluding everything else | `tedge/main/device/#` | " | `tedge/main/#` |
| * All thin-edge device and services data excluding child devices | `tedge/main/+/#` | " | `tedge/main/#` + List of `tedge/<service-id>/#` [^1] |
| * A specific service data | `tedge/<device-id>/service/<service-id>/#` | " | `tedge/<service-id>/#` |
| * A specific child device data including services | `tedge/<child-id>/+/#` | `tedge/<parent-id>/<child-id>/+/#` | `tedge/<child-id>/#` + List of `tedge/<service-id>/#`
| * A specific child device data excluding services | `tedge/<child-id>/device/#` | `tedge/<parent-id>/<child-id>/device/+/#` | `tedge/<child-id>/#`
| * All data from a given child device and its descendants | List of `tedge/<child-id>/+/#` | List of `tedge/<parent-id>/<child-id>/+/+/#` | List of `tedge/<child-id>/#` + List of `tedge/<service-id>/#` on those devices |
| * All data from all immediate child devices of a parent | List of `tedge/<child-id>/+/#` | `tedge/<parent-child-id>/+/+/#` | List of `tedge/<child-id>/#` + List of `tedge/<service-id>/#` on those devices [^2] |
| * Data from all child devices excluding main | List of `tedge/<child-id>/device/#` | List of `tedge/<parent-id>/<child-id>/device/+/#` | List of `tedge/<child-id>/#` |
| --- | --- | --- | --- |
| **_Good-to-Have_** |
| Source context in message | Yes | Yes | No |
| Within the topic limit of AWS | Barely | Exceeds | Easily |

[^1] The list implies a lookup into the inventory of the main device to find all its services and create the filter with the list of those IDs
[^2] First find the list of all children of the given parent and then find the list of services on all of them

## Enhancements

* Introduce a `tedge/self` topic that can be used by developers to write context-agnostic tedge components,
  without worrying about device IDs and their context (whether deployed on main or child device).
  The `tedge/self` prefix must be mapped to `tedge/main` on the main device and `tedge/<child-id>` on the child devices.
  The "connection mechanism" (e.g `tedge connect` on main) can define this as a static mqtt routing rule on each device.
