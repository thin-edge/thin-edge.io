# Exposing configuration to external processes

* Date: __2026-07-03__
* Status: __Draft__

## Background

%%te%% configuration lives in files (`tedge.toml`, per-mapper `mapper.toml`)
read only by the components that own them.
An external process — a custom plugin, or a %%te%% component in its own container —
has no access: the config directory and device certificate are deliberately not
mounted into other containers, and `tedge config get` needs both.
So even a non-secret, widely-needed value like `device.id` is unreachable.

This document proposes exposing **selected** configuration values to such processes,
covering both core settings (`device.id`, local MQTT/HTTP endpoints)
and per-mapper settings (`c8y.url`).

## Goals

* Read selected configuration values without access to the config files or the certificate.
* Safe by default: never expose secrets, even accidentally.
* Respect ownership: no component becomes responsible for configuration it does not own.
* Offer both push (react to values) and pull (ask on demand).

## Design

### Ownership: each component exposes only what it owns

* the **agent** exposes core/device settings (`device.id`, the local MQTT/HTTP endpoints);
* each **mapper** exposes its own cloud settings (`c8y.device.id`, `c8y.mqtt`, `az.url`).

No component reads another's file to expose it on its behalf:
the agent does **not** read the mappers' `mapper.toml`,
which would couple it to every cloud's configuration shape.
The source of truth for each setting stays with the component that owns it.

### Exposure model: opt-in, not opt-out

Configuration holds secrets (`device.key_pin`, `cryptoki.pin`, credential-file paths),
so the exposed set must be curated — by allowlist (opt-in) or denylist (opt-out).

We choose **opt-in**, for *safe by default*.
With a denylist, a newly added setting is exposed the moment it is introduced,
and a new secret leaks unless someone remembers to deny it — a silent failure.
With an allowlist, a new setting is invisible until a maintainer marks it exposable,
so the worst case is a missing value (loud, easy to fix), not a leaked secret.
Marking exposability where each setting is defined also keeps the decision
reviewable alongside the value.

### Distribution: push over MQTT, pull over HTTP

#### Push — retained MQTT (primary).**
Each owner publishes each exposable value as a **retained** message,
one value per topic under its own service topic — mirroring the per-fragment twin-data model.
Per-value topics let a consumer subscribe to exactly the value(s) it needs
and give clean per-value updates (empty retained payload = removal);
retention keeps a value available while its publisher is down.

The topic adds a `config` channel with the configuration key as its final segment:

```
te/device/<device>/service/<service>/config/<key>
```

`<key>` is the configuration key as `tedge config` uses it, and the (retained)
payload is the value as a plain string. For example:

```
te/device/main/service/tedge-agent/config/device.id            -> "my-device-01"
te/device/main/service/tedge-agent/config/mqtt.client.port     -> "1883"
te/device/main/service/tedge-agent/config/http.client.port     -> "8000"
te/device/main/service/tedge-mapper-c8y/config/url             -> "example.cumulocity.com"
te/device/main/service/tedge-mapper-c8y/config/device.id       -> "my-device-01"
te/device/main/service/tedge-mapper-c8y-edge/config/url        -> "edge.c8y.io"
```

Subscribe to a single topic for one value, `.../config/+` for everything a
service exposes, or `te/+/+/+/+/config/+` for every exposed value on the device.

#### Pull — HTTP (convenience view).**
The agent aggregates these retained topics — subscribing like any other consumer,
not reading the mappers' files — and serves each service's config as a
sub-resource of that service's entity, exactly as it already serves twin data:

| Method & path | Result |
|---|---|
| `GET /te/v1/entities/<service>/config` | That service's exposed values, as a JSON object of key to value. |
| `GET /te/v1/entities/<service>/config/<key>` | A single value. |

`<service>` is the service's entity topic-id (e.g. `device/main/service/tedge-agent`),
so the HTTP and MQTT views share the same per-service scoping and the same keys —
no cross-service flattening or re-qualification.
A value that is not exposed (or does not exist) returns `404 Not Found`.
A different error code can be used for non-exposed keys, if that distinction matters.

```console
$ curl http://tedge:8000/te/v1/entities/device/main/service/tedge-agent/config/device.id
my-device-01

$ curl http://tedge:8000/te/v1/entities/device/main/service/tedge-mapper-c8y/config
{"url":"example.cumulocity.com","device.id":"my-device-01"}

$ curl -s -o /dev/null -w '%{http_code}' \
    http://tedge:8000/te/v1/entities/device/main/service/tedge-agent/config/device.key_pin
404
```

## Security considerations

* Only marked values are ever published or served; secrets are never marked.
* Unexposed and non-existent are indistinguishable (no topic / `404`),
  so the boundary reveals nothing about which secret settings exist.
* Retained messages are overwritable by any local client — a tampering nuisance,
  not disclosure; owners republish the authoritative value on change/restart.
  Stronger guarantees come from broker ACLs and the agent's existing TLS/client-cert options.

## Alternatives considered

* **Agent reads all `mapper.toml` files** and exposes everything centrally —
  couples the agent to every cloud's configuration shape and profiles, breaking the ownership rule.
* **Opt-out (denylist) exposure** — not safe by default (see above).
* **A single aggregate config topic per component** instead of one value per topic —
  diverges from the per-fragment twin model and forces consumers to fetch and parse
  the whole document to read one value.
* **Splitting the key across topic segments** (`config/device/id` rather than
  `config/device.id`) — the key would then have a variable depth and need a bespoke
  topic↔key grammar to round-trip back to a `tedge config` key.
  Keeping the whole key as the single final segment is the verbatim `tedge config` key
  and trivially reversible.
* **A single flat `GET /te/v1/config` path** combining all services — collides keys
  published by different services (both the agent and the c8y mapper publish
  `device.id`), loses provenance, and forces the agent to re-qualify mapper keys
  into a global scheme, coupling it to each mapper's naming.
