# Entity Deregister API

* Date: __2024-10-01__
* Status: __New__

## Background

The existing MQTT entity registration API `v1` which expects entities to publish their registration messages
as retained messages to their respective entity topic ids has some limitations.
This design expects entities not to conflict with each other in terms of their topic ids or `@id` values.
And since there is no direct feedback to these messages, the publishing entities need to just hope that it just works.
This expectation might be okay in a very controlled environment where the person provisioning the devices in the field
has complete control over all the devices to avoid any conflicts and even avoid any mistakes (conflicting ids).
But, in environments where that is not feasible, the current model without feedback can have unpleasant side effects.

For example, if an entity registers itself with topic id: `te/device/child01//` and `@id`: `child01`,
a device twin with external id `child01` is created in the cloud and all of its data is routed to this twin.
If another device accidentally registers itself using the same topic id, but with `@id`: `child02`,
a second device twin is created in the cloud with the external id: `child02`
and all the messages published to that topic by `child02` and even `child01` are routed to the new device twin.
The `child01` twin would remain orphaned in the cloud, without the user noticing anything immediately, for lack of feedback.

To avoid such issues, the registering entities must be made aware of any conflicts/errors immediately
so that they don't proceed publishing any data to the wrong entity assuming that it succeeded.
In this case, %%te&& must have rejected the second registration message,
unless it was explicitly done to correct a mistake in the previous registration.

## Challenges

* In the current architecture, the mapper handles all the registration messages and maintains the entity store.
  So, in a deployment with multiple mappers, there would be multiple entity stores maintained by each mapper.
  This results in unnecessary duplication of data and makes it error-prone with multiple sources of truth.
  A central component like `tedge-agent` would be a better fit to store and maintain the entity store.
* The current entity store also maintains the external id of devices, which is a cloud-specific aspect.
  When the entity store is moved to the `tedge-agent`, this aspect must be detached and left with the mappers,
  without the mappers having to maintain a duplicate entity store.
* When the `tedge-agent` handles the registration of entities, it can't consult with the mappers to make sure that
  the entities that it accepted locally are valid in the cloud as well.
  If the local registration succeeds, but the cloud registration fails, the user must get that feedback as well.
* A mapper might get connected much later, after a lot of local registrations are already processed.
  Even in such cases, the mapper should be able to fetch all the entities that were registered till then,
  and replicate that in the cloud.
  Even in that case, any failed registrations must be conveyed to the user asynchronously.

## Solution Proposal

### Proposal 1: REST APIs for registration

* The `tedge-agent` maintains the entity store and provides REST APIs for registration, and querying.
  The mappers and other components can query entity data from it when needed.
* On every successful registration, `tedge-agent` publishes those registration messages to their respective topics,
  so that other components likes mappers subscribed to those topics are notified of those new entities.
* The `tedge-agent` can persist the entity store on disk, as it is done today, for faster recovery on restart.
  It also helps in differentiating already seen entities from newly published ones.
* The mappers no longer maintains the entire entity store but only maintains a simple mapping
  between topic ids and their cloud external ids, so that mapped messages can be published to the same.
* The mappers can rebuild their external id map on every restart, or even persist the same on disk for faster recovery.
* When the registration of a local entity fails in the cloud for reasons like conflicting external ids,
  the corresponding error message from the cloud is forwarded to `te/errors` topic so that the user is aware.

#### Create new entity

**Endpoint**

```
POST /v1/entity
```

**Payload**

```json
{
    "@topic-id": "device/child01//",
    "@type": "child-device",
    "@id": "child01",
    "name": "child01",
    "extra-fragment": {
        "extra-key": "extra-value"
    }
}
```

**Responses**

* 201: Created
  ```json
  {
      "@topic-id": "device/child01//"
  }
  ```
* 409: Conflict
  ```json
  {
      "error": "Entity with topic-id: 'device/child01//' already exists."
  }
  ```

#### Update existing entity

**Endpoint**

```
PUT /v1/entity/{topic-id}
```

**Payload**

```json
{
    "type": "Raspberry Pi",
    "new-fragment": {
        "new-key": "new-value"
    }
}
```

**Responses**

* 200: OK
  ```json
  {
      "@topic-id": "device/child01//",
      "message": "Entity updated successfully."
  }
  ```
* 404: Not Found
  ```json
  {
      "error": "Entity with given topic-id: 'device/child01//' not found."
  }
  ``` 

#### Delete entity

**Endpoint**

```
DELETE /v1/entity/{topic-id}
```

**Responses**

* 200: OK
  ```json
  {
      "@topic-id": "device/child01//",
      "message": "Entity deleted successfully."
  }
  ```
* 404: Not Found
  ```json
  {
      "error": "Entity with given topic-id: 'device/child01//' not found."
  }
  ```

#### Query entity by id

**Endpoint**

```
GET /v1/entity/{topic-id}
```

**Responses**

* 200: OK
  ```json
  {
      "@topic-id": "device/child01//",
      "@id": "child01",
      "name": "child01",
      "type": "Raspberry Pi",
      "extra-fragment": {
          "extra-key": "extra-value"
      },
      "new-fragment": {
          "new-key": "new-value"
      }
  }
  ```
* 404: Not Found
  ```json
  {
      "error": "Entity with given topic id: 'device/child01//' not found."
  }
  ``` 

#### Query multiple entities

All entities:

```
GET /v1/entities
```

**Query Parameters**

By name:

```
GET /v1/entities?name=child01
```

By name and type:

```
GET /v1/entities?name=child01&type=Raspberry%20Pi
```

Pagination keys:

GET /v1/entities?page=1&pageSize=10

**Responses**

* 200: OK
  ```json
  "entities": [
      {
          "@topic-id": "device/child01//",
          "@type": "child-device",
          "@id": "child01",
          "@parent": "device/main//",
          "name": "child01",
          "type": "Raspberry Pi",
          "extra-fragment": {
              "extra-key": "extra-value"
          },
          "new-fragment": {
              "new-key": "new-value"
          }
      },
      {
          "@topic-id": "device/child02//",
          "@type": "child-device",
          "@parent": "device/main//",
          "@id": "child02",
          "name": "child02"
      },
      {
          "@topic-id": "device/child02/service/service01",
          "@type": "service",
          "@parent": "device/child02//",
          "@id": "service01",
          "name": "service01"
      },
      ...
  ],
  "page": 1,
  "pageSize": 10,
  "totalPages" 7
  ```

:::warning
Supporting filtering queries and paginated results would require the `tedge-agent` to maintain
additional data structures like indexes and other things which are yet to be explored/defined.
:::

#### Query child entities of an entity

TBD

#### Delete child entities of an entity

TBD
