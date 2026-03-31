---
title: User-defined mappers
tags: [Reference, Mappers, Cloud]
sidebar_position: 2
---

## Overview

In addition to the built-in cloud mappers (Cumulocity, Azure, AWS), thin-edge supports user-defined mappers that can bridge a device to any IoT cloud platform with an MQTT interface.

A user-defined mapper is a directory under `/etc/tedge/mappers/` that contains a `mapper.toml` configuration file.
Starting the mapper with `tedge-mapper <name>` will look for this directory and run the mapper using the flows engine and/or MQTT bridge configured within it.

## Directory layout

Each mapper lives in its own directory under `/etc/tedge/mappers/`:

```
/etc/tedge/mappers/
└── thingsboard/          ← mapper name
    ├── mapper.toml       ← required: mapper configuration
    ├── bridge/           ← optional: MQTT bridge rules
    │   └── rules.toml
    └── flows/            ← optional: message transformation flows
        └── telemetry.toml
```

The mapper name must:

- start with a lowercase ASCII letter (`a`–`z`)
- contain only lowercase letters, digits, and hyphens (`-`)
- not contain underscores or uppercase letters
- not start with `bridge-` (reserved for internal bridge sub-service naming)

Valid names: `thingsboard`, `my-cloud`, `edge-platform`

Invalid names: `ThingsBoard`, `my_cloud`, `1cloud`, `bridge-example`

### `mapper.toml`

The `mapper.toml` file in the mapper directory configures the cloud broker connection.
It is required for user-defined mappers but optional if the mapper only uses the flows engine without an MQTT bridge.

A minimal `mapper.toml` that connects to a ThingsBoard broker using certificate auth:

```toml
url = "mqtt.thingsboard.io:8883"

[device]
cert_path = "cert.pem"         # relative paths resolved to mapper dir
key_path  = "key.pem"
```

A full example with all supported fields:

```toml
# Cloud broker URL — required for MQTT bridge
url = "mqtt.example.com:8883"

# Authentication: auto (default), certificate, or password
auth_method = "auto"

# Path to a credentials file for username/password auth
# credentials_path = "credentials.toml"

[device]
# MQTT client ID (defaults to the certificate CN if not set)
# id = "my-device"

# Client certificate for mutual TLS.
# Relative paths are resolved relative to the mapper directory.
# Falls back to tedge.toml device.cert_path / device.key_path when absent.
cert_path = "cert.pem"
key_path  = "key.pem"

# CA certificate for verifying the cloud broker's TLS certificate.
# Defaults to the system trust store when absent.
# root_cert_path = "/usr/share/ca-certificates/my-cloud-ca.pem"

[bridge]
# Whether to use a clean MQTT session (default: false)
clean_session = false

# MQTT keepalive interval (e.g. "60s", "2m")
# keepalive_interval = "60s"

# TLS transport control: "auto" (default), "on", or "off".
# auto: infer from port (8883 → TLS on, 1883 → TLS off, other → TLS on)
# on: always use TLS regardless of port
# off: never use TLS (plain TCP, incompatible with certificate authentication)
# tls.enable = "auto"

# Any additional fields you add here are available as ${mapper.*} in bridge rules.
# For example, this field is accessible as ${mapper.bridge.topic_prefix}.
topic_prefix = "v1/devices/me"
```

#### Template variables (`${mapper.*}`)

The entire `mapper.toml` is available for template expansion in bridge rule files via the `${mapper.*}` namespace.
This means you can reference any field — including user-defined ones — directly in your bridge rules without repeating values:

```toml
# bridge/rules.toml — uses ${mapper.bridge.topic_prefix} from mapper.toml
remote_prefix = "${mapper.bridge.topic_prefix}/"

[[rule]]
topic = "telemetry"
direction = "outbound"
```

See [Configurable bridge: `${mapper.*}`](./configurable-bridge.md#mapper--mapperlocal-config) for details.

#### Relative paths

All path fields in `mapper.toml` (`device.cert_path`, `device.key_path`, `device.root_cert_path`, `credentials_path`) support relative paths.
Relative paths are resolved relative to the **mapper directory**, not the process working directory.
For example, `cert_path = "cert.pem"` in `/etc/tedge/mappers/thingsboard/mapper.toml` resolves to `/etc/tedge/mappers/thingsboard/cert.pem`.

Absolute paths are returned unchanged.

#### Certificate fallback

When `device.cert_path` and `device.key_path` are absent from `mapper.toml`, the mapper falls back to `device.cert_path` and `device.key_path` from the root `tedge.toml`.
Explicit `mapper.toml` values always take precedence.

#### `cloud_type` field

The `cloud_type` field identifies which built-in cloud integration this mapper belongs to.
Valid values are: `c8y`, `az`, `aws`.

For custom cloud platforms (e.g. ThingsBoard), you should not set this field.

```toml
cloud_type = "c8y"           # opt into Cumulocity built-in integration
# INVALID: cloud_type = "thingsboard" - non built-in mappers should not specify this field
```

## Starting a user-defined mapper

```sh
sudo tedge-mapper thingsboard
```

Or with systemd (after creating a service file for `tedge-mapper-thingsboard`):

```sh
sudo systemctl start tedge-mapper-thingsboard
```

The corresponding service name is `tedge-mapper-<name>`.

## `tedge mapper` commands

The `tedge mapper` subcommand of the `tedge` CLI provides utilities for inspecting user-defined mappers configured on the device.
This is distinct from `tedge-mapper`, which is the mapper daemon binary that actually runs a mapper process.

### `tedge mapper list`

Lists all configured mappers under `/etc/tedge/mappers/`, along with their URL, device identity, and `cloud_type` if set.
Directories that contain a `mapper.toml` or a `flows/` subdirectory are included.

```sh
tedge mapper list
```

```text title="Example output"
c8y mqtt.cumulocity.com:8883 my-device [tedge.toml] c8y
thingsboard mqtt.thingsboard.io:8883 my-device [mapper.toml]
```

Output columns are tab-separated: **name**, **URL**, **device ID** (with source annotation), and **cloud_type**.

### `tedge mapper config get`

Reads a configuration value from a mapper's `mapper.toml` using a dotted key path:

```sh
tedge mapper config get thingsboard.url
```

```text title="Example output"
mqtt.thingsboard.io:8883
```

Nested keys use dot notation:

```sh
tedge mapper config get thingsboard.device.cert_path
```

```text title="Example output"
/etc/tedge/mappers/thingsboard/cert.pem
```

## Example: ThingsBoard

The following example illustrates how to structure a ThingsBoard mapper.
It is intended to show how the pieces fit together, not as a ready-to-run ThingsBoard integration.
For a maintained, working example see the [tedge-flows-examples](https://github.com/thin-edge/tedge-flows-examples) repository.

### 1. Create the mapper directory

```sh
sudo mkdir -p /etc/tedge/mappers/thingsboard
```

### 2. Create `mapper.toml`

```sh
sudo tee /etc/tedge/mappers/thingsboard/mapper.toml <<'EOF'
url = "mqtt.thingsboard.io:8883"

[device]
cert_path = "cert.pem"   # relative to /etc/tedge/mappers/thingsboard/
key_path  = "key.pem"

[bridge]
# Custom field — available as ${mapper.bridge.topic_prefix} in bridge rules
topic_prefix = "v1/devices/me"
EOF
```

### 3. Add bridge rules

```sh
sudo mkdir -p /etc/tedge/mappers/thingsboard/bridge
sudo tee /etc/tedge/mappers/thingsboard/bridge/telemetry.toml <<'EOF'
remote_prefix = "${mapper.bridge.topic_prefix}/"

[[rule]]
local_prefix = "tb/"
topic = "telemetry"
direction = "outbound"

[[rule]]
local_prefix = "tb/"
topic = "rpc/request/#"
direction = "inbound"
EOF
```

See [Configurable bridge](./configurable-bridge.md) for the full bridge rule syntax, and [tedge-flows-examples](https://github.com/thin-edge/tedge-flows-examples) for complete, maintained flows examples.

### 4. Create a service file and start the mapper

Create a systemd service file for the mapper (or the equivalent for your init system):

```sh
sudo tee /etc/systemd/system/tedge-mapper-thingsboard.service <<'EOF'
[Unit]
Description=thin-edge.io user-defined mapper thingsboard
After=syslog.target network.target mosquitto.service

[Service]
User=tedge
ExecStart=/usr/bin/tedge-mapper thingsboard
Restart=on-failure
RestartPreventExitStatus=255
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
sudo systemctl daemon-reload
sudo systemctl enable --now tedge-mapper-thingsboard
```
