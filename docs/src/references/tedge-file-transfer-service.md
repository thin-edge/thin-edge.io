---
title: File Transfer Service
tags: [Reference, HTTP]
sidebar_position: 6
---

# Thin Edge File Transfer Service

The `tedge-agent` hosts a binary repository for child devices and other plugins/extensions to exchange binary files between them.
This repository is meant to be used as a temporary storage for exchanging files, and not for storing items permanently,
as the storage on thin-edge devices are typically very limited.

Files can be uploaded, downloaded and deleted from this repository via the following HTTP endpoints:

|Type|Method|Endpoint|
|----|------|--------|
|Upload|PUT|http://{tedge-ip}:8000/tedge/file-transfer/{path}/{to}/{resource}|
|Download|GET|http://{tedge-ip}:8000/tedge/file-transfer/{path}/{to}/{resource}|
|Delete|DELETE|http://{tedge-ip}:8000/tedge/file-transfer/{path}/{to}/{resource}|

The `tedge-ip` is derived from the following tedge configurations:

* mqtt.bind.address
* mqtt.external.bind.address

If the `mqtt.external.bind.address` is configured, then the `tedge-ip` is set to that value,
else the `mqtt.bind.address` is used with the default value of `127.0.0.1`.

The files uploaded to this repository are stored at `/var/tedge/file-transfer` directory.
The `{path}/{to}/{resource}` specified in the URL is replicated under this directory.

For example, a file uploaded to `http://{tedge-ip}/tedge/file-transfer/config_update/mosquitto/mosquitto.conf`
is stored at `/var/tedge/file-transfer/config_update/mosquitto/mosquitto.conf`.

An existing file at a given path is replaced on subsequent uploads using the same URL path.
Unique paths must be used in the URL path to avoid such overwrites.

All uploaded files are preserved until they are explicitly deleted with the DELETE API.
To avoid exhaustion of storage space on the thin-edge device,
users must be diligent to delete any stored files as soon as their purpose is served.
