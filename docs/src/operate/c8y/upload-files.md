---
title: Uploading Files
tags: [Operate, Cumulocity, Operation]
description: Uploading Files
---

%%te%% provides the `tedge upload c8y` command to upload a file from the device to the cloud.

```sh
tedge upload c8y --file /path/to/some/file
```

- creates a new event on the device tenant
- uploads the file to Cumulocity, attaching this file to the event
- and returns the event ID for further processing by the invoking script.

```text title="Output: the Cumulocity ID of the new event"
151205
```

The `tedge upload c8y` determines the MIME type of the file content using the file extension.
If no rules applies, then `application/octet-stream` is taken as a default.
When these rules are not appropriate, the MIME type of the file content can be explicitly set.

```sh
--mime-type <MIME_TYPE>
    MIME type of the file content
    
    If not provided, the mime type is determined from the file extension
    If no rules apply, application/octet-stream is taken as a default
```

## Setting the event properties

The properties of the event created by the `tedge upload c8y` command can be set on the command line:

```sh
--type <EVENT_TYPE>
  Type of the event
  
  [default: tedge_UploadedFile]

--text <TEXT>
  Text description of the event. Defaults to "Uploaded file: <FILE>"

--json <JSON>
  JSON fragment attached to the event
  
  [default: {}]
```

## Uploading a file from a child device

The `tedge upload c8y` can also be used from a child device or on behalf of a service.

For that to work, one has to provide the child device / service identifier as registered on Cumulocity

```sh
--device-id <DEVICE_ID>
  Cumulocity external id of the device/service on which the file has to be attached.
  
  If not given, the file is attached to the main device.
```

## Prerequisites

Under the hood the `tedge upload c8y` use the local HTTP proxy between the device and its Cumulocity tenant,
notably delegating to this proxy authentication and JWT token handling.

This implies that:

- The device must be connected to its tenant, i.e `tedge connect c8y --test` must be successful.
- The c8y mapper must be running on the main device, enabling the local HTTP proxy to Cumulocity.
- To upload a file from a child device the proxy must be accessible from the child device.
  - The default for the proxy is to listen only on the loopback address (`127.0.0.1`),
    meaning that the default settings have to be changed for a child device to use the proxy.
  - On the main device, both the bind address of the proxy (`c8y.proxy.bind.address`)
    and the client address `c8y.proxy.client.host` have to be set to the IP address of the main device.
  - On the child devices, the client address `c8y.proxy.client.host` has to be set accordingly.
  - proxy listens to the IP address and port configured by `c8y.proxy.bind.address` and `c8y.proxy.bind.port`.
  - Run `tedge config list --doc proxy` for the full list of options.
