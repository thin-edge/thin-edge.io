# Entity Deregister API

* Date: __2024-03-14__
* Status: __New__

## Background

All the entities (devices and services) registered with %%te%% are stored with the MQTT broker as retained messages,
with their metadata spread across multiple topics.
For example, for a child device `child01`, the registration message is stored on the `te/device/child01//` topic,
its twin data is stored across several `te/device/child01///twin/<twin-data-type>` topics,
its command metadata stored across several `te/device/child01///cmd/<cmd-type>` topics and so on.

On top of that, when a device has services associated with it, or has nested child devices,
they all have their own respective registration topics and respective metadata topics.
So, deregistering a device involves deregistering itself, its metadata and
the complete entity hierarchy that is associated with it.

## Challenges

* Metadata and data associated with a given entity spread across multiple topics, must be cleared together.
* When services and child devices are linked to a device, deleting that entity involves deleting all linked entities,
  to avoid leaving orphan entities behind.
* The parent and child devices may not share a common topic prefix, making it difficult to do it purely from an MQTT level.
  For example, a device `te/device/child01//` may have an associated service `te/device/child01/service/service01`
  and nested child devices `te/device/child11//` and `te/device/child12//`.
  Even though the parent device and service has a common prefix `te/device/child01/#`, there is no such common prefix
  between the parent device and its child devices.
  And when custom topic schemes are used, even the device and its service may not have any common prefix available.

## Finalized Solution Proposal

Entities can be deregistered by clearing their registration message from the broker, publishing empty retained messages to their topic id, as follows:

```sh
tedge mqtt pub -r te/device/child01// ''
```

This message will result in the deregistration of that target entity and all the nested entities (services and child devices) linked to it.

Different `tedge` components will react to the deregistration messages as follows:

**tedge-agent**

* The `tedge-agent` identifies the entire nested entity hierarchy of the target entity and issue clear messages for all of them, starting from the leaf nodes.
* A parent entity must be cleared from the broker only after all its children are cleared, so that even if the agent crashes in the middle of a deregistration,
  it can resume on restart.

:::note
To easily identify the target entity hierarchy to be deregistered, the `tedge-agent` also needs to maintain its own entity store, which it doesn't currently do.
:::

**tedge-mapper**

* The `tedge-mapper` clears the entity metadata of the target entity and its children (services and child devices) from its own entity store.
* Since deregistration messages for different entities could be received out of order, a child's clear message could be received after the parent is cleared.
  In such cases, simply ignore the clear message for that non-existent entity.
* Propagating the deregistration to the cloud (removing the managed object from the cloud) is controlled by the config setting: `c8y.sync.push_deregister_upstream`.
* When the `c8y.sync.push_deregister_upstream` setting is set to `true`, the corresponding managed object is deleted from the cloud using the
  [Delete managed object REST API](https://cumulocity.com/api/core/#operation/deleteManagedObjectResource).
  If the config setting value is `false`, the managed object in the cloud is left untouched.

### Limitations

This solution has the following limitations:

1. There is no feedback available to the user when the deregistration of an entire device hierarchy is complete, especially the child entities.
   Although the deregistration of the target entity itself can be validated by checking if its registration message is cleared from the broker,
   the same can't be done for the child entities as there is no API currently available to query the list of all the children of an entity.
1. No filtering capabilities available for more fine-grained deregistration (e.g: deregister only the services/children of a given entity).

Both these limitations can be alleviated in future by introducing new commands that lets users register/deregister entities and even list them
based on some filtering criteria.
Since commands inherently supports status updates, the status of operations like `register` and `deregister` can also be published via the same.
This command based approach is documented in one of the deferred proposals [here](#mqtt-api-3).

## Appendix (Comparison of all the proposals)

### Using third-party MQTT tools like mosquitto_sub

The `mosquitto_sub` command provides options like `--remove-retained` to selectively remove all retained messages in a topic hierarchy by using topic filters.
For example, the following command can be used to remove all retained messages under the `te/device/child01` topic tree:

```sh
mosquitto_sub --remove-retained -t "te/device/child01/+/+/#"
```

The mappers must also be updated to remove entities from their entity store on receipt of these clear messages.

**Pros**

* Existing out-of-the-box solution.

**Cons**

* Works only for topic schemes that ensures that parent and child entities have a common topic prefix.
* Even the default topic scheme ensures common topic prefix only for a device and its services and not for its nested children.
  So, it only works for simple deployments that does not have nested child devices.

### Tedge deregister command

Provide a `tedge` command that takes the topic id of the entity to be deregistered as follows:

```sh
tedge deregister te/device/child01//
```

This command subscribes to all entity messages, recreates the entity store on its own so that it can identify
the given entity and all of its linked entities so that they can all be deregistered.

:::note
Subscribing to the given entity topic tree as `te/device/child01/#` isn't enough to identify all the linked entities
as they may not share a common topic prefix.
:::

Once all the target entities are identified, it issues secondary subscriptions for all the data topics of these entities,
like their twin data, alarm and command topics so that all those retained message are also cleared.
On receipt of these clear messages, the mappers must clear the corresponding entries from their own in-memory entity stores,
and propagate that to the cloud.

The following options are provided to enable more fine-grained deregistration of entities:
* `--services-only`: Delete only the associated services of the given device and not the device itself.
* `--children-only`: Delete only the nested child devices not the device itself.
* `--child-type`: Delete all the child devices with the given type

**Pros**

* Offers better targeting of entities, than what can be done using topic filters.

**Cons**

* Deregistrations can't be done remotely, unless `tedge` is installed on those remote device as well.

### MQTT API 1

Deregister entities by publishing MQTT requests as follows:

```sh
tedge mqtt pub te/device/child01///req/deregister '{"children-only": true}'
```

The request payload supports various filtering options as follows:
* `children-only`
* `services-only`
* `child-type` 

...where an empty payload means deregister the target device as well as its linked entities.

The `tedge-agent` handles these requests exactly as done by the `tedge deregister` command proposed above,
by clearing all the retained messages associated with the target entities.
The mappers react the same way as proposed in the previous solution.

The responsibility of issuing the clear messages is left with the agent instead of the mapper,
which already has its own entity store, to prevent multiple mappers from doing the same simultaneously.
So, the `tedge-agent` also needs to maintain an entity store, or create one on the fly, to easily identify the targets.
Since deregistrations are rare operations, creating it on the fly and discarding it after use is also acceptable.

**Pros**

* Allows these commands to be triggered from any remote device over MQTT without needing `tedge` to be installed.
* Scope to support filtering options which can be added/expanded in future.

**Cons**

* No immediate feedback on the status of the deregister request unless the status is published to a corresponding response topic,
  which is difficult to manage for clients.
* Not symmetric with registration message
* No support for resumption on partial failure 

### MQTT API 2

By publishing empty retained message to the entity topic id as follows:

```sh
tedge mqtt pub -r te/device/child01// ''
```

**Pros**

* Symmetric with the registration API by using the same topic

**Cons**

* The status response will have to be sent on an entirely different topic, which breaks the symmetry anyway.
* If the mapper crashes in the middle of the deregistration of a deeply nested hierarchy of nested devices,
  resumption on restart would be difficult as there is no trace of a pending request as the retained message is cleared.
* Further API expansion to support more filtering options not possible

### MQTT API 3

Deregister using commands as follows:

```sh
tedge mqtt pub -r te/device/child01///cmd/deregister/id-1234 '{"status": "init", "children-only": true}'
```

The agent would react to this command as described in the previous solution and send the final `successful` or `failed` status.

More commands like `list`, that can be used to fetch the list of child devices, services or both can also be added in future.

**Pros**

* Using existing `cmd` mechanism which already covers responses as well.
* Since commands are resumable by nature, deregistration can also be resumed in the event of a crash.
* Allows future expansions to include more filtering parameters as well.

**Cons**

* Not symmetric with registration message

### HTTP API

Entity store HTTP APIs exposed by the `tedge-agent` as follows:

* Method: `DELETE`
* URL: `/tedge/entity-store/<entity-topic-id-without-trailing-slashes>`
* Parameters:
  * `services-only` (optional)
  * `children-only` (optional)
  * `child-type` (optional)
* Response:
  * Code:
    * 200: Successfully deleted the entity
    * 204: Entity not found, hence nothing to delete
    * 401: Unauthorized access
  * Body: List of topic IDs of deleted entities
* Response Body:
* Examples:
  * DELETE http:://<tedge-host>/tedge/entity-store/device/child1
  * DELETE http:://<tedge-host>/tedge/entity-store/device/child1/service/service1
  * DELETE http:://<tedge-host>/tedge/entity-store/device/child1?services-only=true

The HTTP requests are handled by the `tedge-agent` exactly the same way as done for the MQTT API proposal.
The mappers must also react the same way to the clear commands.

**Pros**

* Can be triggered remotely.
* Immediate feedback available on the status of the operation.
* Web GUI friendly.

**Cons**

* Inconsistency with the registration API which is available only via MQTT
