---
title: 🚧 Entity management REST APIs
tags: [Child-Device, Registration]
sidebar_position: 1
description: Register child devices and services with %%te%%
draft: true
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

## Delete entity

Deleting an entity results in the deletion of its immediate and nested child entities as well, to avoid leaving orphans behind.
The deleted entities are cleared from the MQTT broker as well.

**Endpoint**

```
DELETE /v1/entities/{topic-id}
```

**Responses**

* 200: OK
  ```json
  ["device/child01//"]
  ```

**Example**

```shell
curl -X DELETE http://localhost:8000/tedge/entity-store/v1/entities/device/child01
```
