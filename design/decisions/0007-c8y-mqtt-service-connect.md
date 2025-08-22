# Connect to C8Y MQTT service endpoint

* Date: __2025-06-23__
* Status: __Approved__

## Requirement

Cumulocity has introduced a new [MQTT service endpoint](https://cumulocity.com/docs/device-integration/mqtt-service)
that allows users to send free-form messages (any arbitrary topic and payload) to Cumulocity.
It does not replace the existing core MQTT endpoint and hence a separate connection is required.
To support the same, `tedge connect` must be enhanced to connect to this new endpoint as well,
in addition to the existing connection to the core endpoint.

## MQTT service specifications/limitations

- The mqtt service endpoint is exposed at the same tenant URL via port 9883 (with TLS).
- Both username/password based and cert based authentication is supported.
- Only connections with clean session enabled are supported. Persistent connection attempts are rejected.
- Subscribing to wildcard topics (`#` and `+`) and system topics (starting with `$SYS`) are not supported.
- Publishing/subscribing to legacy C8Y topics (SmartREST and JSON over MQTT) are not supported either.
- QoS 2 is not supported.
- Retained flag is accepted, but ignored.
- There are limits to the number of topics that can be created
  and the number of messages that can be queued per topic.
  Publishes are rejected when the queue is full.
- Each published message has a time-to-live, after which it is cleared from the queue.
  The subscribed clients are not notified of expired messages.

## Solutions

To establish a bridge connection to the mqtt service endpoint, the following info must be provided in the connect request:

* host: Same as legacy mqtt endpoint
* port: 9883 by default
* client id: device id (but need not be the device id itself)
* auth:
  * for username/password based authentication
    * username: `<tenant_id>/<user_name>`
    * password
  * for cert based authentication
    * device cert path
    * device key path
    * server root ca path
* bridge topics: `<bridge_topic_prefix>/#`

The following are the solution approaches that were considered.

### Approach 1: Connect to mqtt service along with core mqtt connection

:::info
This approach was finalized and implemented due to the smooth transition that it offers
when Core MQTT is replaced by the MQTT service in the future.
:::

The mqtt service bridge connection is established along with `tedge connect c8y` command.

:::note
For the builtin bridge functionality, whether to spawn a different mapper,
or establish a secondary connection from the existing mapper itself is to be decided.
The same applies for the trivial mapping rule that adds the timestamp to the payload.
:::

#### Configuration

The mqtt service connection related configs are defined under the `[c8y]` table in `tedge.toml` :

| Config | Description | Default |
| ------ | ----------- | ------- |
| `c8y.mqtt_service.enabled` | Whether the mqtt server connection must be established or not | `false` |
| `c8y.mqtt_service.url` | The config URL for the MQTT service | `<c8y.url>:9883` |
| `c8y.mqtt_service.topic_prefix` | topic prefix used to bridge mqtt topics | `c8y-mqtt` |
| `c8y.mqtt_service.topics` | topics to subscribe to on the mqtt service | `$debug/$error` |

To support connecting exclusively to the mqtt service, excluding the core mqtt connection,
a config flag (`c8y.core_mqtt.enabled`) to enable/disable the core mqtt connection can also be introduced.


To connect to the mqtt service, it must be enabled before `tedge connect c8y` is executed:

```sh
tedge config set c8y.mqtt_service.enabled "true"
```

#### Connection

The command to connect to C8Y remains the same:

```sh
tedge connect c8y
```

The connection to the mqtt service is established only if it is enabled.

**Pros**

- Makes the mqtt service connection seamless for existing users, with minimal configuration.
- Reuse most of the existing configurations like `c8y.url`, `c8y.device.key_path`, `c8y.device.cert_path` etc.
- Users can choose **not** to connect to the new endpoint, by keeping it disabled.
- In future, if the core mqtt connection is replaced with the new mqtt service,
  we can just switch what is enabled and disabled by default.

**Cons***

- The mqtt service connection is tied to the direct `c8y` connection, to retrieve the tenant id
  (although this dependency can be removed if tenant id is accepted via config).

### Approach 2: Connect to mqtt service as an independent cloud endpoint

Do not establish both the core mqtt connection and the mqtt service connection together with `tedge connect c8y` command,
but treat the mqtt service as an independent/generic MQTT broker endpoint like connecting to AWS or AZ endpoints.

#### Configuration

As an independent cloud endpoint, the following config values must be provided:

```sh
tedge config set remote.url example.cumulocity.com:9883
tedge config set remote.bridge.topic_prefix c8y-mqtt
```

The following setting can also be configured, though they all have default values:

| Config | Description | Default |
| ------ | ----------- | ------- |
| `remote.device.cert_path` | The device cert path used for the authentication | Same as `device.cert_path` |
| `remote.device.key_path` | The device cert path used for the authentication | Same as `device.key_path` |

In addition to the above, pretty much all the configs defined in the `tedge.toml` for `az` and `aws` can be defined
for this generic `remote` endpoint as well, as none of them are cloud specific but applies to any mqtt broker.

#### Connection

```sh
tedge connect remote
```

**Pros**

- Clear separation from `tedge connect c8y` allows the second connection to be established and disconnected on demand.
- Existing users are not affected, as the `tedge connect c8y` behavior stays the same.
- Allows customers to connect exclusively to the new mqtt service, if they don't need the core mqtt connection.
- There is no real dependency on Cumulocity as multiple `remote` profiles can be used to connect to any generic broker.

**Cons**

- Many of the config parameters defined in the `c8y` profile like the `url`, `cert_path` etc
  will have to be duplicated in `remote` profile as well.
- When the device is connected to multiple c8y cloud profiles,
  corresponding `remote` profiles must be configured explicitly per c8y instance.

### Approach 3: Connect to all configured cloud instances with tedge connect

This is more of an extension of `Approach 2`, where instead of explicitly connecting to each cloud instance/profile,
a `tedge connect` command (without the cloud arg), detects all the cloud profiles that are configured/enabled
and connects to each endpoint one by one.

Only the cloud instances with a `url` explicitly configured are considered as enabled.
An additional `enabled` flag can be added to each cloud profile, to skip connecting to it, even when it is configured.

#### Configuration

TBD

#### Connection

TBD

