---
title: Entity Management
tags: [Registration, Deregistration]
sidebar_position: 11
---

import DocCardList from '@theme/DocCardList';

%%te%% provides REST APIs to manage all the entities (child devices and services) attached to a main device.
Entity management includes:
- Create/Register entities with metadata describing their role and relationship.
- Fetch the metadata of existing entities
- Update the metadata of existing entities
- Delete/Deregister registered entities

%%te%% provides two different flavors of this API:

- HTTP API: For RESTful communication suitable for request/response patterns.
- MQTT API: For event-driven communication using publish/subscribe patterns.

Both APIs provide access to the same core functionality but are optimized for different use cases.
Choose the most appropriate API based on your specific requirements:
- Use HTTP API for traditional request/response interactions, where the feedback is important
- Use MQTT API for real-time event-driven interactions

For example,the realtime notifications on entity changes make the MQTT APIs ideal for such use-cases,
whereas the inherent feedback in the HTTP APIs make it ideal for operations like querying, deregistration etc,
where the status of the operation is key.

%%te%% also provides an entity [auto-registration mechanism](./auto-registration.md) that automatically registers entities
on receipt of the very first message from them, without waiting for an explicit registration.
This is critical for simple devices like sensors that just emits telemetry data,
but does not support an entity registration logic to be programmed into them.

<DocCardList />
