---
title: SmartREST 1.0 and basic auth
tags: [Operate, Cumulocity]
description: Establishing connection with Basic auth and using SmartREST 1.0
---

%%te%% supports basic authentication and [SmartREST 1.0.](https://cumulocity.com/docs/smartrest/smartrest-one/)
This page explains how to use basic authentication and SmartREST 1.0 with Cumulocity.

:::important
It is highly recommended to use certificate-based authentication and SmartREST 2.0.
This guide is intended for users who must use the SmartREST 1.0 for specific reasons.
:::

## Setting up basic authentication

To use SmartREST 1.0, the authentication mode must be set to `basic` using the `tedge config` CLI tool:

```sh
sudo tedge config set c8y.auth_method basic
```

Next, provide credentials (username/password) in a credential file formatted as follows.
The default location of the credentials file is `/etc/tedge/credentials.toml`:

```toml title="file: /etc/tedge/credentials.toml"
[c8y]
username = "t5678/octocat"
password = "abcd1234"
```

If needed, you can specify a custom location for the credentials file using the `tedge config` CLI tool:

```sh
sudo tedge config set c8y.credentials_path /custom/path/to/credentials.toml
```

## Configuring the device ID

The device ID must be explicitly set. This ID will be used as the external ID of the device in your Cumulocity tenant.

```sh
sudo tedge config set device.id <id>
```

## Adding SmartREST 1.0 template to %%te%% config

You need to specify all the external IDs of the SmartREST 1.0 templates you plan to use.
This can be achieved by providing a comma-separated list of the external IDs:

```sh
sudo tedge config set c8y.smartrest1.templates template-1,template-2
```

Alternatively, you can add the template IDs one by one:

```sh
sudo tedge config add c8y.smartrest1.templates template-1
sudo tedge config add c8y.smartrest1.templates template-2
```

## Connecting to Cumulocity

Before connecting to Cumulocity, ensure the following prerequisites are met:

- The device has already been registered in Cumulocity using the [Bulk Registration API](https://cumulocity.com/docs/device-management-application/registering-devices/#bulk-device-registration).
- The SmartREST 1.0 templates have been registered in your Cumulocity tenant.

Once these steps are complete, you can connect to Cumulocity:

```sh
sudo tedge connect c8y
```
