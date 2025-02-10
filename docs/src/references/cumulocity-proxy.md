---
title: Cumulocity Proxy
tags: [ Reference, HTTP, Cumulocity ]
sidebar_position: 12
description: Using the Cumulocity Proxy for full access to Cumulocity API
---

The `tedge-mapper` (when running in `c8y` mode) hosts a proxy server to access the Cumulocity HTTP API from the
%%te%% device.
It automatically handles authenticating with a JWT, avoiding the need for clients to support MQTT to retrieve this
information.

The Cumulocity HTTP API can be accessed at `http://{host}:{port}/c8y/{c8y-endpoint}`. Configuration settings
`c8y.proxy.client.host` and `c8y.proxy.client.port` are used to configure `{host}` and `{port}` parts of the base URL
which will be used by %%te%% components to make requests to the C8y Proxy. `c8y.proxy.bind.address` and
`c8y.proxy.bind.port` are read by `tedge-mapper-c8y` and used as bind address and port for the Cumulocity HTTP proxy. In
both `client` and `bind`, `127.0.0.1` and `8001` are used as defaults for the address and port, respectively.

For example, you can access the current tenant information
at [http://127.0.0.1:8001/c8y/tenant/currentTenant](http://127.0.0.1:8001/c8y/tenant/currentTenant)
from the machine running `tedge-mapper`.
The server supports all public REST APIs of Cumulocity, and all possible request methods
(e.g. `HEAD`/`GET`/`PUT`/`PATCH`/`DELETE`).
There is no need to provide an `Authorization` header (or any other authentication method) when accessing the API.
If an `Authorization` header is provided, this will be used to authenticate the request instead of the device JWT.

## HTTPS and authenticated access
By default, the service is unauthenticated  and does not support incoming HTTPS connections
(when the request is forwarded to Cumulocity, however, this will use HTTPS).
HTTPS can be enabled by setting `c8y.proxy.cert_path` and `c8y.proxy.key_path`.
If the certificates are configured, the mapper will automatically host the proxy via HTTPS, and redirect any
HTTP requests to the equivalent HTTPS URL.
If HTTPS is enabled, the configured certificate should be installed in the OS trust store for any connected agents
in order for them to trust the connection to the mapper.

Once HTTPS is enabled for the mapper, certificate-based authentication can also be enabled.
The directory containing the certificates that the mapper will trust can be configured using `c8y.proxy.ca_path`,
and the agent can be configured to use a trusted certificate using the `http.client.auth.cert_file` and `http.client.auth.key_file`
settings.

## Possible errors returned by the proxy
Due to the underlying JWT handling in Cumulocity, requests to the proxy API are occasionally spuriously rejected with
a `401 Not Authorized` status code.
The proxy server currently forwards this response directly to the client, as well as all other errors responses from
Cumulocity.
If there is an error connecting to Cumulocity to make the request, a plain text response with the status
code `502 Bad Gateway` will be returned.

## Using tedge http

[`tedge http`](../references/cli/tedge-http.md) can be used to access Cumulocity from any child devices,
provided [proper configuration](../references/cli/tedge-http.md#configuration).

For example, you can access the current tenant information
from the machine running `tedge-mapper` as well as any child device:

```sh title="Interacting with Cumulocity"
   tedge http get /c8y/tenant/currentTenant
```

