---
title: Legacy
tags: [Legacy]
sidebar_position: 9
---

import DocCardList from '@theme/DocCardList';

Thin-edge 1.0 introduced a set of breaking changes that might affect plugins and extensions implemented on previous versions.

In most cases, a compatibility layer has been introduced to smooth the transition.
For instance, any measurement published by an extension on the former topic `tedge/measurement`
is republished by the **tedge-agent** to the topic `te/device/main///m/`
which is dedicated to untyped measurements for the main device.

However, the compatibility layers don't address all the breaking changes and, in any case, they will be deprecated medium-term.
Here are the developer guides to port a legacy extension to the new thin-edge API. 

<DocCardList />