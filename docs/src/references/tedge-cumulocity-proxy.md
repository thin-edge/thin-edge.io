---
title: Thin Edge Cumulocity HTTP Proxy
tags: [ Reference, HTTP, Cumulocity ]
sidebar_position: 12
---

# Thin Edge Cumulocity Proxy

The `tedge-mapper` (when running in `c8y` mode) hosts a proxy server to access the Cumulocity HTTP API from the
thin-edge device.
It automatically handles authenticating with a JWT, avoiding the need for clients to support MQTT to retrieve this
information.

The API can be accessed at `http://{ip}:8001/c8y/{c8y-endpoint}`. `ip` is configured using the `c8y.proxy.bind.address`
configuration
(the default value is `127.0.0.1`) and the port can be changed by setting `c8y.proxy.bind.port`.

For example, you can access the current tenant information
at [http://127.0.0.1:8001/c8y/tenant/currentTenant](http://127.0.0.1:8001/c8y/tenant/currentTenant)
from the machine running `tedge-mapper`.
The server supports all public REST APIs of Cumulocity, and all possible request methods
(e.g. `HEAD`/`GET`/`PUT`/`PATCH`/`DELETE`).
There is no need to provide an `Authorization` header (or any other authentication method) when accessing the API.
If an `Authorization` header is provided, this will be used to authenticate the request instead of the device JWT.

At the time of writing, this service is unauthenticated and does not support incoming HTTPS connections
(when the request is forwarded to Cumulocity, however, this will use HTTPS).
Due to the underlying JWT handling in Cumulocity, requests to the proxy API are occasionally spuriously rejected with
a `401 Not Authorized` status code.
The proxy server currently forwards this response directly to the client, as well as all other errors responses from
Cumulocity.
If there is an error connecting to Cumulocity to make the request, a plain text response with the status
code `502 Bad Gateway` will be returned.
