# Connecting an external device to `thin-edge.io`

With `thin-edge.io` you can enable connection for external devices to your `thin-edge.io` enabled device with the use of a few commands.

> Note: Currently, only one additional listener can be defined.

## Configuration

External devices connection can be setup by using the `tedge` cli tool making some changes to the configuration.

The following configurations option are available for you if you want to add an external listener to thin-edge.io:

`mqtt.external.port`             Mqtt broker port, which is used by the external mqtt clients to publish or subscribe. Example: 8883
`mqtt.external.bind_address`     IP address / hostname, which the mqtt broker limits incoming connections on. Example: 0.0.0.0
`mqtt.external.bind_interface`   Name of network interface, which the mqtt broker limits incoming connections on. Example: wlan0

`mqtt.external.capath`           Path to a file containing the PEM encoded CA certificates that are trusted when checking incoming client certificates. Example: /etc/ssl/certs
`mqtt.external.certfile`         Path to the certificate file, which is used by external MQTT listener. Example: /etc/tedge/server-certs/tedge-certificate.pem
`mqtt.external.keyfile`          Path to the private key file, which is used by external MQTT listener. Example: /etc/tedge/server-certs/tedge-private-key.pem

> If none of these options is set, then no external listener is set.
> If one of these options is set, then default values are inferred by the MQTT server (Mosquitto). For instance, the port defaults to 1883 for a non-TLS listener, and to 8883 for a TLS listener.

These settings can be considered in 2 groups, listener configuration and TLS configuration.

### Configure basic listener

To configure basic listener you should provide port and/or bind address which will use default interface.
To change the default interface you can use mqtt.external.bind_interface configuration option.

To set them you can use `tedge config` as so:

```shell
tedge config set mqtt.external.port 8883
```

To allow connections from all IP addresses on the interface:

```shell
tedge config set mqtt.external.bind_address 0.0.0.0
```

### Configure TLS on the listener

To configure the external listener with TLS additional settings are available: `mqtt.external.capath` `mqtt.external.certfile` `mqtt.external.keyfile`

To enable MQTT over TLS, a server side certificate must be configured using the 2 following settings:

```shell
tedge config set mqtt.external.certfile /etc/tedge/server-certs/tedge-certificate.pem
tedge config set mqtt.external.keyfile /etc/tedge/server-certs/tedge-private-key.pem
```

To fully enable TLS authentication clients, client side certificate validation can be enabled:

```shell
tedge config set mqtt.external.capath /etc/ssl/certs
```

> Note: Providing all 3 configuration will trigger thin-edge.io to require client certificates.
