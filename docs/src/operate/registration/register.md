---
title: Entity management REST APIs
tags: [Child-Device, Registration]
sidebar_position: 1
description: Register child devices and services with %%te%%
---

# REST APIs for Entity Management

In addition to the MQTT APIs, %%te%% now supports the following REST APIs as well to manage entities with it:
* Creation
* Retrieval
* Deletion

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
curl http://localhost:8000/tedge/entity-store/v1/entities \
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
curl http://localhost:8000/tedge/entity-store/v1/entities/device/child01
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
  ```
  Entity not found with topic id: device/unknown/topic/id
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
curl http://localhost:8000/tedge/entity-store/v1/entities
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
curl http://localhost:8000/tedge/entity-store/v1/entities?root=device/child2//
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
curl http://localhost:8000/tedge/entity-store/v1/entities?parent=device/child2//
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
curl http://localhost:8000/tedge/entity-store/v1/entities?type=child-device
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
curl 'http://localhost:8000/tedge/entity-store/v1/entities?parent=device/child2//&type=service'
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

## Delete entity

Deleting an entity results in the deletion of its immediate and nested child entities as well, to avoid leaving orphans behind.
The deleted entities are cleared from the MQTT broker as well.

**Endpoint**

```
DELETE /v1/entities/{topic-id}
```

**Example**

```shell
curl -X DELETE http://localhost:8000/tedge/entity-store/v1/entities/device/child21
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
