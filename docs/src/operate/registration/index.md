---
title: Entity Management
tags: [Registration, Deregistration]
sidebar_position: 11
---

import DocCardList from '@theme/DocCardList';

%%te%% provides two different API interfaces for managing entities on a device:

- HTTP API: For RESTful communication suitable for request/response patterns.
- MQTT API: For event-driven communication using publish/subscribe patterns.

Both APIs provide access to the same core functionality but are optimized for different use cases.
Choose the most appropriate API based on your specific requirements:
- Use HTTP API for traditional request/response interactions, where the feedback is important
- Use MQTT API for real-time event-driven interactions

For example, the inherent feedback in the HTTP APIs make it ideal for operations like querying, deregistration etc.

<DocCardList />
