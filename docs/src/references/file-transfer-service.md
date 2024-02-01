---
title: File Transfer Service
tags: [Reference, HTTP]
sidebar_position: 6
descrioption: Interacting with the %%te%% file transfer service
---

The `tedge-agent` hosts a binary repository for child devices and other plugins/extensions to exchange binary files between them.
This repository is meant to be used as a temporary storage for exchanging files, and not for storing items permanently,
as the storage on %%te%% devices are typically very limited.

Files can be uploaded, downloaded and deleted from this repository via the following HTTP endpoints:

|Type|Method|Endpoint|
|----|------|--------|
|Upload|PUT|`http://{fts-address}:8000/tedge/file-transfer/{path}/{to}/{resource}`|
|Download|GET|`http://{fts-address}:8000/tedge/file-transfer/{path}/{to}/{resource}`|
|Delete|DELETE|`http://{fts-address}:8000/tedge/file-transfer/{path}/{to}/{resource}`|

The `fts-address` is derived from `http.client.host` config setting with a default value of `127.0.0.1`.

The files uploaded to this repository are stored at `/var/tedge/file-transfer` directory.
The `{path}/{to}/{resource}` specified in the URL is replicated under this directory.

For example, a file uploaded to `http://{fts-address}/tedge/file-transfer/config_update/mosquitto/mosquitto.conf`
is stored at `/var/tedge/file-transfer/config_update/mosquitto/mosquitto.conf`.

An existing file at a given path is replaced on subsequent uploads using the same URL path.
Unique paths must be used in the URL path to avoid such overwrites.

All uploaded files are preserved until they are explicitly deleted with the DELETE API.
To avoid exhaustion of storage space on the %%te%% device,
users must be diligent to delete any stored files as soon as their purpose is served.

## HTTPS and authenticated access
By default, the service is unauthenticated and does not support HTTPS connections.
HTTPS can be enabled by setting `http.cert_path` and `http.key_path`.
If the certificates are configured, the agent will automatically host the service via HTTPS, and redirect any
HTTP requests to the equivalent HTTPS URL.
If HTTPS is enabled, the configured certificate should be installed in the OS trust store for any connected agents
in order for them to trust the connection to the mapper.

Once HTTPS is enabled for the file-transfer service, certificate-based authentication can also be enabled.
The directory containing the certificates that the agent will trust can be configured using `http.ca_path`,
and the mapper as well as the child device agents can be configured to use a trusted certificate using the
`http.client.auth.cert_file` and `http.client.auth.key_file` settings.
