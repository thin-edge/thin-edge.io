## Memory statistics

Run mapper:

```sh
env MAPPER_MQTT_HOST=test.mosquitto.org cargo run --release --features memory-statistics
```

Produce load:

```sh
mosquitto_pub -h test.mosquitto.org -t tedge/measurements --repeat 10000 -q 2 -m '{"time": "2020-10-15T05:30:47+00:00", "temperature": 25, "coordinate": {"x": 32.54,"y": -117.67,"z": 98.6},"pressure": 98 }'
```

Measure memory:

```sh
mosquitto_sub -h test.mosquitto.org -t 'SYS/mapper/bytes/allocated'
```

