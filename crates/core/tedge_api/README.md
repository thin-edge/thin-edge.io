# Motivation

The point here is to play with the idea of using actors to implement the core components of thin-edge.
The main question is:

> How to re-organise the current `tedge_mapper` and `tedge_agent`
> around a small set of actors
> exchanging typed-messages via in-memory channels?

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│  tuned mapper + agent                                         │
│                                                               │
│    ┌────────────────┐                ┌─────────────────┐      │
│    │                │                │                 │      │
│    │ c8y plugin     ├─► operations───►  apt plugin     │      │
│    │                │                │                 │      │
│    │                │                │                 │      │
│    │                │                │                 │      │
│    └────▲─┬───▲─────┘                └─────────────────┘      │
│         │ │   │                                               │
│         │ │   │                      ┌─────────────────┐      │
│         │ │   │                      │                 │      │
│         │ │   └────────telemetry ◄───┤ thin-edge JSON  │      │
│         │ │                          │                 │      │
│         │ │                          │                 │      │
│         │ │                          │                 │      │
│         │ │                          └───────▲─────────┘      │
│         │ │                                  │                │
│         │ │                                  │                │
│         │ │                                  │                │
│   ┌─────┴─▼──────────────────────────────────┴────────────┐   │
│   │                                                       │   │
│   │  MQTT Connection plugin                               │   │
│   │                                                       │   │
│   │                                                       │   │
│   │                                                       │   │
│   └─────▲─┬──────────────────────────────────▲────────────┘   │
│         │ │                                  │                │
│         │ │                                  │                │
└─────────┼─┼──────────────────────────────────┼────────────────┘
          │ │                                  │
          │ │                                  │
          │ │                                  │
          │ ▼                                  │


          C8y                                 Sensors
```

* This PR focuses on the building blocks and their relationships.
* There is no attempt to implement any concrete feature beyond printing the received messages.
* This PR doesn't address the critical issue of configuration and dynamic instantiation of plugins.
  * A use-case for a dynamic instantiation is to instantiate a plugin for a cloud *only* when configured.
  * This is addressed by [this other rfc](https://github.com/thin-edge/thin-edge.io/pull/979).
  

