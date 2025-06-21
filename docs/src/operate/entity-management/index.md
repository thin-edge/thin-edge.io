---
title: Entity Management
tags: [Registration, Deregistration]
sidebar_position: 11
---

import DocCardList from '@theme/DocCardList';

%%te%% provides APIs to manage all the entities (child devices and services) attached to a main device.
- Create/Register entities with metadata describing their role and relationship.
- Read/Fetch entity metadata
- Update entity metadata
- Delete/Deregister entities

%%te%% provides two different flavors of this API, with the same core functionality but optimized for different use cases
- HTTP API: For request/response interactions where actual responses drive subsequent requests (e.g. start sending telemetry data only when the registration is complete)
- MQTT API: For event-driven interactions where the point is to notify and react to changes (e.g. maintain in the cloud a twin representation of the device with its services and child devices) 

%%te%% also provides an entity [auto-registration mechanism](./auto-registration.md) that automatically registers entities
on receipt of the very first message from them, without waiting for an explicit registration.
This is critical for simple devices like sensors that just emits telemetry data,
but does not support an entity registration logic to be programmed into them.

<DocCardList />
