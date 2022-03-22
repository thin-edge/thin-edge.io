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
* This has been done with a top-down approach starting for the [target `main.rs`](examples/tedge_c8y.rs).
* The main requirements are:
  * A thin-edge executable is built using a coarse-grain plugins all made using the same kind of building blocks.
  * Each plugin is defined by a configuration (here defined at compile-time, in practice read from disk on start).
  * The plugins are instantiated from their config in an uniform manner.
  * The plugins are then connected via typed channels.
  * Finally, the assemblage of plugins is started.  
  * In this example:
    * A `c8y` collects `Measurement`s, translates them into c8y messages and forwards the translations to c8y via MQTT.
    * The measurements are produced by two plugins: `collectd` and `thin_edge_json`. Both are reading the raw measurements via MQTT.
    * The `c8y` plugin also received messages from c8y via MQTT. These messages are translated into software management requests.
    * The software management requests are processed by a main `sm_service` plugin that dispatches the requests to the appropriate plugin.
    * Two software package managers plugin, `apt` and `apama`, process the software management requests for these software types.
  * A key point is that all the channels are typed and that these types are not defined by thin-edge but by the plugins.
    * The tedge_api crate defines only the assemblage rules.
    * Message types, e.g. MqttMessage or Measurement, are defined by specific plugin crates.
       *  For instance, `MqttMessage`s can be defined in a `tedge_mqtt_plugin` crate along an `MqttConnection` plugin
          to consume and produce message over an MQTT connection.
       *  `Measurement`, `Alarm`, `Event` will be defined in a `tedge_telemetry` crate.
       *  A `thin_edge_json_plugin` crate can the defined to exchange telemetry data over MQTT.
          This crate will depend on the two formers.
  
  

