---
title: Proxy Configuration
tags: [Operate, Proxy, MQTT, HTTP]
description: Connecting %%te%% to a cloud via a proxy server
---

As of version 1.5.0, %%te%% can connect to a proxy server (for both HTTP and MQTT connections).

The protocol for MQTT connections is HTTP CONNECT. This establishes an initial connection with HTTP, then connects via TCP as usual for MQTT.

Proxy servers are not supported by the mosquitto bridge, so the built-in bridge must be enabled in order to connect %%te%% via a proxy server.

By default, the configured proxy is used to connect to the cloud MQTT service and for all HTTP communication.

## Configuring the proxy server

To configure %%te%% with a proxy server, you need an address and (optionally) a username and password.

```shell
tedge config set mqtt.bridge.built_in true # Required for MQTT proxying
tedge config set proxy.address https://example.com:8080
tedge config set proxy.username user
tedge config set proxy.password pass
tedge config set proxy.no_proxy 127.0.0.1 # Skip the proxy for connections to 127.0.0.1
```

Once you have configured the proxy, you need to (re)connect to your chosen cloud(s):

```shell
tedge reconnect c8y # or aws/az
```

`tedge connect` will confirm the configured proxy server URL in the summary information it shows.

Once the proxy server is configured, `tedge-agent` will need to be restarted to ensure the proxy configuration is respected.
