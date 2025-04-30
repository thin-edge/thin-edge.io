---
title: Entity management REST APIs
tags: [Child-Device, Registration]
sidebar_position: 1
description: Register child devices and services with %%te%%
---

# REST APIs for Entity Management

%%te%% provides REST APIs to manage all the entities (devices and services) attached to a main device.
These APIs let you create, retrieve, update and delete entities.

When compared to the MQTT APIs, the REST APIs provide an added advantage of immediate feedback to the user,
if the respective call succeeded or not, which is lacking with the MQTT APIs.
For example, when a new entity registration is attempted, if a conflicting entity with the same topic id exists already,
the registration attempt fails and the feedback is provided immediately in the HTTP response.

## Create new entity

Successful creation of an entity results in its definition getting published to the MQTT broker as well.

**Endpoint**

```
POST /v1/entities
```

**Payload**

```json
{
    "@topic-id": "device/child01//",
    "@type": "child-device",
    "@id": "child01",
    "name": "child01"
}
```

**Responses**

* 201: Created
  ```json
  {
      "@topic-id": "device/child01//"
  }
  ```

**Example**

```bash
curl http://localhost:8000/te/v1/entities \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "@topic-id": "device/child01//",
    "@type": "child-device",
    "name": "child01"
  }'
```

## Fetch entity definition

**Endpoint**

```
GET /v1/entities/{topic-id}
```

**Responses**

* 200: OK
  ```json
  {
      "@topic-id": "device/child01//",
      "@id": "child01",
      "name": "child01"
  }
  ```

**Example**

```shell
curl http://localhost:8000/te/v1/entities/device/child01
```

## Query entities

**Endpoint**

```
GET /v1/entities
```

**Query parameters**

| Parameter | Description                                                            | Examples                            |
|-----------|------------------------------------------------------------------------|-------------------------------------|
| `root`    | Entity tree starting from the given `root` node (including it) | `device/child2//`                   |
| `parent`  | Direct child entities of the given `parent` entity (excluding it)             | `device/main//`                     |
| `type`    | Entities of the given entity `type`                          | `main`, `child-device` or `service` |

The following restrictions apply:
* Multiple values can not be specified for the same parameter.
* The same parameter can not be repeated multiple times.
* The `root` and `parent` parameters can not be used together.


**Responses**

* 200: OK
  ```json
  [
      {
          "@topic-id": "device/main//",
          "@type": "device"
      },
      {
          "@topic-id": "device/main/service/service0",
          "@type": "service",
          "@parent": "device/main//"
      },
      {
          "@topic-id": "device/child0//",
          "@type": "child-device",
          "@parent": "device/main//"
      }
      ...
  ]
  ```
* 404: Not Found
  ```json
  {
      "error": "Entity with topic id: device/unknown// not found"
  }
  ```

**Examples**

To demonstrate different query examples, the following entity tree is assumed as the base:

```
main
|-- service0
|-- service1
|-- child0
|   |-- child00
|   |   |-- child000
|-- child1
|   |-- service10
|-- child2
|   |-- service20
|   |-- child20
|   |   |-- child200
|   |-- child21
|   |   |-- service210
|   |   |-- child210
|   |-- child22
```

### Query all entities

List all entities registered with thin-edge starting from the `main` device at the root.

**Request**

```shell
curl http://localhost:8000/te/v1/entities
```

**Response**

```json
[
    {
        "@topic-id": "device/main//",
        "@type": "device"
    },
    {
        "@topic-id": "device/main/service/service0",
        "@type": "service",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/main/service/service1",
        "@type": "service",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child0//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child1//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child2//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child00//",
        "@type": "child-device",
        "@parent": "device/child0//"
    },
    {
        "@topic-id": "device/child1/service/service10",
        "@type": "service",
        "@parent": "device/child1//"
    },
    {
        "@topic-id": "device/child2/service/service20",
        "@type": "service",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child20//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child21//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child22//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child200//",
        "@type": "child-device",
        "@parent": "device/child20//"
    },
    {
        "@topic-id": "device/child21/service/service210",
        "@type": "service",
        "@parent": "device/child21//"
    },
    {
        "@topic-id": "device/child210//",
        "@type": "child-device",
        "@parent": "device/child21//"
    }
]
```

### Query from a root

Query the entity tree from a given root node.

**Request**

```shell
curl http://localhost:8000/te/v1/entities?root=device/child2//
```

**Response**

```json
[
    {
        "@topic-id": "device/child2//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child2/service/service20",
        "@type": "service",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child20//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child21//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child22//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child200//",
        "@type": "child-device",
        "@parent": "device/child20//"
    },
    {
        "@topic-id": "device/child21/service/service210",
        "@type": "service",
        "@parent": "device/child21//"
    },
    {
        "@topic-id": "device/child210//",
        "@type": "child-device",
        "@parent": "device/child21//"
    }
]
```

### Query by parent

Query only the immediate child entities of a `parent`, excluding any nested entities.

**Request**

```shell
curl http://localhost:8000/te/v1/entities?parent=device/child2//
```

**Response**

```json
[
    {
        "@topic-id": "device/child2/service/service20",
        "@type": "service",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child20//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child21//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child22//",
        "@type": "child-device",
        "@parent": "device/child2//"
    }
]
```

### Query by type

Query all entities of type: `child-device`

**Request**

```shell
curl http://localhost:8000/te/v1/entities?type=child-device
```

**Response**

```json
[
    {
        "@topic-id": "device/child0//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child1//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child2//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child00//",
        "@type": "child-device",
        "@parent": "device/child0//"
    },
    {
        "@topic-id": "device/child20//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child21//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child22//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child200//",
        "@type": "child-device",
        "@parent": "device/child20//"
    },
    {
        "@topic-id": "device/child210//",
        "@type": "child-device",
        "@parent": "device/child21//"
    }
]
```

### Query with multiple parameters

Query all child services of the parent: `device/child2//`.

**Request**

```shell
curl 'http://localhost:8000/te/v1/entities?parent=device/child2//&type=service'
```

**Response**

```json
[
    {
        "@topic-id": "device/child2/service/service20",
        "@type": "service",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child21/service/service210",
        "@type": "service",
        "@parent": "device/child21//"
    }
]
```
## Update entity

An existing entity can be updated using the PATCH API.
But the updates are limited to the `@parent` and `@health` endpoints only.
Other properties like `@type` and `@id` can not be updated after the registration.

The `@parent` must be another device, as services cannot have children.
Similarly, the `@health` endpoint must be a service, as only services publish their health statuses.
The entities specified in either fields must be registered up-front, before they can be used.

**Endpoint**

```
PATCH /v1/entities/{topic-id}
```

**Payload**

```json
{
    "@parent": "{new-parent-topic-id}",
    "@health": "{some-service-topic-id}"
}
```

**Example**

```bash
curl http://localhost:8000/te/v1/entities/device/child01 \
  -X PATCH \
  -H "Content-Type: application/json" \
  -d '{"@parent": "device/child0//"}'
```

**Responses**

* 200: OK, with the updated entity definition on a successful update
  ```json
  {
      "@topic-id": "device/child01//",
      "@parent": "device/child0//",
      "@type": "child-device"
  }
  ```

## Delete entity

Deleting an entity results in the deletion of its immediate and nested child entities as well, to avoid leaving orphans behind.
The deleted entities are cleared from the MQTT broker as well.

**Endpoint**

```
DELETE /v1/entities/{topic-id}
```

**Example**

```shell
curl -X DELETE http://localhost:8000/te/v1/entities/device/child21
```

**Responses**

* 200: OK, when entities are deleted
  ```json
  [
    {
        "@topic-id": "device/child21//",
        "@type": "child-device",
        "@parent": "device/child2//"
    },
    {
        "@topic-id": "device/child21/service/service210",
        "@type": "service",
        "@parent": "device/child21//"
    },
    {
        "@topic-id": "device/child210//",
        "@type": "child-device",
        "@parent": "device/child21//"
    }
  ]
  ```
* 204: No Content, when nothing is deleted

## Set entity twin data

The twin data fragments for an entity can be set either individually or together in a single message.
The twin data set/deleted with these HTTP APIs are published as retained `twin` messages to the MQTT broker as well.

### Set a single twin fragment

Set a single twin fragment value for an existing entity.

**Endpoint**

```
PUT /v1/entities/{topic-id}/twin/{fragment-key}
```

**Payload**

Any JSON value.

**Examples**

* Set `name` fragment with a `string` value (Additional `"` quotes are required for JSON strings):

  ```shell
  curl http://localhost:8000/te/v1/entities/device/child01///twin/name \
    -X PUT \
    -H "Content-Type: application/json" \
    -d '"Child 01"'
  ```
* Set `maintenanceMode` fragment with a `boolean` value:
  ```shell
  curl http://localhost:8000/te/v1/entities/device/child01///twin/maintenanceMode \
    -X PUT \
    -H "Content-Type: application/json" \
    -d 'true'
  ```
* Set `hardware` fragment with an `object` value:
  ```shell
  curl http://localhost:8000/te/v1/entities/device/child01///twin/hardware \
    -X PUT \
    -H "Content-Type: application/json" \
    -d '{"serialNo": "98761234"}'
  ```

**Responses**

* 200: OK (Return the current value of the twin fragment)
  ```json
  {
      "serialNo": "98761234"
  }
  ```
* 400: Bad Request (When the fragment key starts with the reserved `@` character)
  ```json
  {
      "error": "Invalid twin key: '@id'. Keys that are empty, containing '/' or starting with '@' are not allowed"
  }
  ```
* 404: Not Found
  ```json
  {
      "error": "The specified entity: device/test-child// does not exist in the entity store"
  }
  ```

### Set all twin fragments

Set all entity twin fragments at once.
All previous values are replaced at once with the provided values.

**Endpoint**

```
PUT /v1/entities/{topic-id}/twin
```

**Payload**

Any fragments to be inserted/updated are specified with their desired values.
Fragments to be removed are specified with a `null` value.

```json
{
    "new-fragment": {
        "new-key": "new-value"
    },
    "fragment-to-update": "updated-value",
    "fragment-to-delete": null
}
```

**Example**

Update existing fragment: `name`, add new fragment: `hardware` and remove existing fragment: `maintenanceMode` (with a `null` value):

```shell
curl http://localhost:8000/te/v1/entities/device/child01///twin \
  -X PUT \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Child 01",
    "hardware": {
        "serialNo": "98761234"
    },
    "maintenanceMode": null
  }'
```

**Responses**

* 200: OK
  ```json
  {
      "name": "Child 01",
      "hardware": {
          "serialNo": "98761234"
      },
  }
  ```
* 400: Bad Request (Invalid JSON payload or payload with fragment keys starting with the reserved `@` character)
  ```json
  {
      "error": "Fragment keys starting with '@' are not allowed as twin data"
  }
  ```
* 404: Not Found
  ```json
  {
      "error": "The specified entity: device/test-child// does not exist in the entity store"
  }
  ```

## Get entity twin data

The twin fragments of an entity can be queried individually or as a whole.

### Get a single twin fragment

**Endpoint**

```
GET /v1/entities/{topic-id}/twin/{fragment-key}
```

**Example**

```shell
curl http://localhost:8000/te/v1/entities/device/child01///twin/hardware
```

**Responses**

* 200: OK
  ```json
  {
      "serialNo": "98761234"
  }
  ```
* 404: Not Found
  ```json
  {
      "error": "The specified entity: device/test-child// does not exist in the entity store"
  }
  ```

### Get all twin fragments

**Endpoint**

```
GET /v1/entities/{topic-id}/twin
```

**Example**

```shell
curl http://localhost:8000/te/v1/entities/device/child01///twin
```

**Responses**

* 200: OK
  ```json
  {
      "name": "Child 01",
      "hardware": {
          "serialNo": "98761234"
      },
  }
  ```
* 404: Not Found
  ```json
  {
      "error": "The specified entity: device/test-child// does not exist in the entity store"
  }
  ```

## Delete entity twin data

The twin fragments of an entity can be deleted either individually or all at once.

### Delete a single twin fragment

**Endpoint**

```
DELETE /v1/entities/{topic-id}/twin/{fragment-key}
```

**Example**

```shell
curl -X DELETE http://localhost:8000/te/v1/entities/device/child01///twin/maintenanceMode
```

This is equivalent to using the `PUT` API with a `null` value as follows:

```shell
curl http://localhost:8000/te/v1/entities/device/child01///twin/maintenanceMode \
  -X PUT \
  -H "Content-Type: application/json" \
  -d 'null'
```

**Responses**

* 204: No Content (Whether the target twin data was deleted or did not exist already)

### Delete all twin fragments

**Endpoint**

```
DELETE /v1/entities/{topic-id}/twin
```

**Example**

```shell
curl -X DELETE http://localhost:8000/te/v1/entities/device/child01///twin
```

**Responses**

* 204: No Content (Whether the target twin data was deleted or did not exist already)
