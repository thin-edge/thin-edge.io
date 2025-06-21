---
title: REST API
tags: [Child-Device, Registration]
sidebar_position: 1
description: Register child devices and services with HTTP
---

# REST API for Entity Management

%%te%% provides a REST API to manage all the entities (devices and services) attached to the main device.
These interfaces let you create, retrieve, update and delete entities.

When compared to the MQTT API, the REST API provides an added advantage of immediate feedback to the user,
if the respective call succeeded or not, which is lacking with the MQTT API.
For example, when a new entity registration is attempted, if a conflicting entity with the same topic id exists already,
the registration attempt fails and the feedback is provided immediately in the HTTP response.

## Create a new entity {#create-entity}

To create(register) a new entity, send a `POST` request with the entity definition in the request body.
The payload must contain at least the `@topic-id` and `@type` of the entity: whether it is a `child-device` or `service`.

:::note
The topic id of the entity is not specified in the URL path, but in the payload itself,
unlike the MQTT API where it is specified in the MQTT topic.
:::note

Other supported (optional) fields in the registration payload include:
- `@parent`: Topic ID of the parent entity.
  Required for nested child devices or services where the parent cannot be derived from the topic.
- `@id`: External ID for the entity.
- `@health`: Topic ID of the health endpoint service of this entity.
  Valid only for `child-device` entities.
  By default, it is the `tedge-agent` service on that device.

Successful creation of an entity results in its definition getting published to the local MQTT broker as well.

**Endpoint**

```
POST /te/v1/entities
```

**Payload**

```json
{
    "@topic-id": "device/child01//",
    "@type": "child-device"
}
```

**Response status codes**

* 201: Created

### Example: Create a new child device

Register a child device with the topic-id `device/child0//` and
assign it as the child device of the main device (`device/main//`).
Its unique topic-id will be used to reference it in all other API calls (both for REST and MQTT).

```sh
curl http://localhost:8000/te/v1/entities \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "@topic-id": "device/child0//",
    "@type": "child-device"
  }'
```

```json title="Response"
{
    "@topic-id": "device/child0//"
}
```

### Example: Create a nested child device

Register `device/child01//` as a child device of `device/child0//` which is specified as the `@parent`:

```sh
curl http://localhost:8000/te/v1/entities \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "@topic-id": "device/child1//",
    "@type": "child-device",
    "@parent": "device/child0"
}'
```

### Example: Create a service with initial metadata and twin data

Register `device/child1/service/nodered` as a service of `device/child1//` which is specified as the `@parent`,
along with optional `@id` and initial twin data: `name` and `type`:

```sh
curl http://localhost:8000/te/v1/entities \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{
    "@topic-id": "device/child1/service/nodered",
    "@type": "service",
    "@parent": "device/child1//",
    "@id": "child1-nodered",
    "name": "nodered",
    "type": "RasPi 4"
}'
```

## Get an entity {#get-entity}

Get an entity's metadata (e.g. `@type`, `@parent`, `@id` etc).

**Endpoint**

```
GET /te/v1/entities/{topic-id}
```

**Response status codes**

* 200: OK

### Example: Get a child device

Get the child device `device/child0//`.

```sh
curl http://localhost:8000/te/v1/entities/device/child0
```

```json title="Response"
{
    "@topic-id": "device/child0//",
    "@type": "child-device",
    "@parent": "device/main//"
}
```

:::note
The trailing slashes, `/`, can be omitted from the path for this API request.
:::

## Update an entity {#update-entity}

The PATCH API can be used to:
* change the parent of a child device or a service
* change the health endpoint of a device

Updates are limited to the `@parent` and `@health` properties only,
so other properties like `@type` and `@id` cannot be updated after the registration.

:::note
The `@parent` must be another device, as services cannot have children.
Similarly, the `@health` endpoint must be a service, as only services publish their health statuses.
The entities specified in either fields must be registered up-front, before they can be used.
:::

**Endpoint**

```
PATCH /te/v1/entities/{topic-id}
```

**Payload**

```json
{
    "@parent": "{new-parent-topic-id}",
    "@health": "{some-service-topic-id}"
}
```

**Response status codes**

* 200: OK, with the updated entity definition on a successful update

### Example: Update the parent of an entity

Update the parent of the entity `device/child01//` by making it a child of `device/child0//`:

```sh
curl http://localhost:8000/te/v1/entities/device/child01 \
  -X PATCH \
  -H "Content-Type: application/json" \
  -d '{"@parent": "device/child0//"}'
```

```json title="Response"
{
    "@topic-id": "device/child01//",
    "@parent": "device/child0//",
    "@type": "child-device"
}
```

## Delete an entity {#delete-entity}

Deleting an entity results in the deregistration of that entity and its descendants (immediate and nested child entities) as well,
to avoid leaving orphan entities behind.
The deregistered entities are cleared from the local MQTT broker as well.

**Endpoint**

```
DELETE /te/v1/entities/{topic-id}
```

**Response status codes**

* 200: OK, when entities are deleted
* 204: No Content, when nothing is deleted

### Example: Delete a child device

Remove a child device and any of its children.

```sh
curl -X DELETE http://localhost:8000/te/v1/entities/device/child21
```

```json title="Response"
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

## Update entity twin data

The twin data fragments for an entity can be set either individually or together in a single message.
The twin data set/deleted with the HTTP API are published as retained `twin` messages to the local MQTT broker as well.

### Update a single twin fragment

Set a single twin fragment value for an existing entity.

**Endpoint**

```
PUT /te/v1/entities/{topic-id}/twin/{fragment-key}
```

**Payload**

Any JSON value.

**Response status codes**

* 200: OK (Return the current value of the twin fragment)
* 400: Bad Request (When the fragment key starts with the reserved `@` character)
* 404: Not Found

#### Example: Update the device's name (string)

Update the device's name.

:::note
You'll need to surround the value with double quotes (`"`), in order for the value to be interpreted as a string.
:::

```sh
curl http://localhost:8000/te/v1/entities/device/child01///twin/name \
  -X PUT \
  -H "Content-Type: application/json" \
  -d '"Child 01"'
```

```json title="Response"
"Child 01"
```

#### Example: Update a boolean value

Update the `maintenanceMode` fragment with a boolean value.

```sh
curl http://localhost:8000/te/v1/entities/device/child01///twin/maintenanceMode \
  -X PUT \
  -H "Content-Type: application/json" \
  -d 'true'
```

```json title="Response"
true
```

#### Example: Update a JSON object

Update the `hardware` fragment with a JSON object.

```sh
curl http://localhost:8000/te/v1/entities/device/child01///twin/hardware \
  -X PUT \
  -H "Content-Type: application/json" \
  -d '{"serialNo": "98761234"}'
```

```json title="Response"
{"serialNo": "98761234"}
```

### Update all twin fragments

Set all entity twin fragments at once.
All previous values are replaced at once with the provided values.

**Endpoint**

```
PUT /te/v1/entities/{topic-id}/twin
```

**Payload**

JSON object.

Any fragments to be inserted/updated are specified with their desired values.
If a fragment is given a `null` value, then it will be removed.

**Response status codes**

* 200: OK
* 400: Bad Request (Invalid JSON payload or payload with fragment keys starting with the reserved `@` character)
* 404: Not Found

#### Example: Replace all existing twin data

Update existing fragment: `name`, add new fragment: `hardware` and remove existing fragment: `maintenanceMode` (with a `null` value):

```sh
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

```json title="Response"
{
    "name": "Child 01",
    "hardware": {
        "serialNo": "98761234"
    }
}
```

## Get entity twin data

The twin fragments of an entity can be queried individually or as a whole.

### Get a single twin fragment

Get a single digital twin value for a given entity.

**Endpoint**

```
GET /te/v1/entities/{topic-id}/twin/{fragment-key}
```

**Response status codes**

* 200: OK
* 404: Not Found

#### Example: Get the hardware information

Get the hardware information for the main device.

```sh
curl http://localhost:8000/te/v1/entities/device/main///twin/hardware
```

```json title="Response"
{
    "serialNo": "98761234"
}
```

### Get all twin fragments

Get all of the digital twin values for a given entity.

**Endpoint**

```
GET /te/v1/entities/{topic-id}/twin
```

**Response status codes**

* 200: OK
* 404: Not Found

#### Example: Get all twin data

Get all twin data for the main device.

```sh
curl http://localhost:8000/te/v1/entities/device/main///twin
```

```json title="Response"
{
    "name": "tedge012345",
    "hardware": {
        "serialNo": "98761234"
    }
}
```

## Delete entity twin data

The twin fragments of an entity can be deleted either individually or all at once.

### Delete a single twin fragment

**Endpoint**

```
DELETE /te/v1/entities/{topic-id}/twin/{fragment-key}
```

:::note
Deleting a digital twin fragment can also be done using the `PUT` API and sending `null` in the body:

```sh
curl http://localhost:8000/te/v1/entities/device/child01///twin/maintenanceMode \
  -X PUT \
  -H "Content-Type: application/json" \
  -d 'null'
```
:::

**Response status codes**

* 204: No Content (Whether the target twin data was deleted or did not exist already)

#### Example: Delete a single twin fragment

Delete the `maintenanceMode` fragment from the digital twin data

```sh
curl -f -X DELETE http://localhost:8000/te/v1/entities/device/child01///twin/maintenanceMode
```

### Delete all twin fragments

Delete all digital twin fragments from an entity.

**Endpoint**

```
DELETE /te/v1/entities/{topic-id}/twin
```

**Response status codes**

* 204: No Content (Whether the target twin data was deleted or did not exist already)

#### Example: Delete all twin fragments of a child device

Remove all of the child device's twin data using a single request.

```sh
curl -f -X DELETE http://localhost:8000/te/v1/entities/device/child01///twin
```

## Query entities

Get a list of entities which match given filter criteria.

**Endpoint**

```
GET /te/v1/entities
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


**Response status codes**

* 200: OK
* 404: Not Found

### Examples

To demonstrate different query examples, the following entity tree is assumed as the base:

```
main
|-- tedge-mapper-c8y
|-- tedge-agent
|-- child0
|   |-- child00
|-- child1
|   |-- tedge-agent
|-- child2
|   |-- tedge-agent
|   |-- child20
|   |   |-- child200
|   |-- child21
|   |-- child22
```

#### Example: Query all entities

List all entities registered with thin-edge starting from the `main` device at the root.

**Request**

```sh
curl http://localhost:8000/te/v1/entities
```

```json title="Response"
[
    {
        "@topic-id": "device/main//",
        "@type": "device"
    },
    {
        "@topic-id": "device/main/service/tedge-mapper-c8y",
        "@type": "service",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/main/service/tedge-agent",
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
        "@topic-id": "device/child0/service/tedge-agent",
        "@type": "service",
        "@parent": "device/child0//"
    },
    {
        "@topic-id": "device/child00//",
        "@type": "child-device",
        "@parent": "device/child0//"
    },
    {
        "@topic-id": "device/child1/service/tedge-agent",
        "@type": "service",
        "@parent": "device/child1//"
    },
    {
        "@topic-id": "device/child2/service/tedge-agent",
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
    }
]
```

#### Example: Query from a root

Query the entity tree from a given root node.

**Request**

```sh
curl http://localhost:8000/te/v1/entities?root=device/child2//
```

```json title="Response"
[
    {
        "@topic-id": "device/child2//",
        "@type": "child-device",
        "@parent": "device/main//"
    },
    {
        "@topic-id": "device/child2/service/tedge-agent",
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
    }
]
```

#### Example: Query by parent

Query only the immediate child entities of a `parent`, excluding any nested entities.

**Request**

```sh
curl http://localhost:8000/te/v1/entities?parent=device/child2//
```

```json title="Response"
[
    {
        "@topic-id": "device/child2/service/tedge-agent",
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

#### Example: Query by type

Query all entities of type: `child-device`

**Request**

```sh
curl http://localhost:8000/te/v1/entities?type=child-device
```

```json title="Response"
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
    }
]
```

#### Example: Query with multiple parameters

Query all child services of the parent: `device/child2//`.

**Request**

```sh
curl 'http://localhost:8000/te/v1/entities?parent=device/child2//&type=service'
```

```json title="Response"
[
    {
        "@topic-id": "device/child2/service/tedge-agent",
        "@type": "service",
        "@parent": "device/child2//"
    }
]
```
