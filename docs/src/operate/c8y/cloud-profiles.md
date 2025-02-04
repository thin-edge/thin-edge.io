---
title: Cloud Profiles
tags: [Operate, Cumulocity, Cloud Profile]
description: Connecting %%te%% to multiple Cumulocity tenants
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';

Starting with version 1.4.0, **%%te%%** supports multiple Cumulocity connections
using cloud profiles. This can be useful for migrating from one Cumulocity
tenant to another, where Cloud profiles allow you to configure and run multiple
`tedge-mapper c8y` instances from a single `tedge.toml` configuration file.

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing
relevant settings which will be reflected in the instructions. It makes it even
easier to explore and use %%te%%.

<UserContextForm settings="C8Y_PROFILE_NAME,C8Y_PROFILE_URL,C8Y_URL,DEVICE_ID" />

The user context will be persisted in your web browser's local storage.
:::

## Configuration
There are a few values that need to be configured before we are able to connect
to a second Cumulocity tenant.

### URL
To connect to a second tenant, start by configuring the URL of the new tenant:

<UserContext>
```sh
sudo tedge config set c8y.url $C8Y_PROFILE_URL --profile $C8Y_PROFILE_NAME
```
</UserContext>

The profile name can be any combination of letters and numbers, and is used only
to identify the cloud profile within thin-edge. The names are case insensitive,
so `--profile second` and `--profile SECOND` are equivalent.

You can now see the configuration has been applied to `tedge.toml`:

```sh
tedge config list url
```

<UserContext language="sh" title="Output">

```sh
c8y.url=$C8Y_URL
c8y.profiles.$C8Y_PROFILE_NAME.url=$C8Y_PROFILE_URL
```

</UserContext>

In addition to the URL there are a couple of other configurations that need to
be set for the second mapper:
- the MQTT bridge topic prefix
- the Cumulocity proxy bind port

### MQTT bridge topic prefix
<UserContext>
```sh
sudo tedge config set c8y.bridge.topic_prefix c8y-$C8Y_PROFILE_NAME --profile $C8Y_PROFILE_NAME
```
</UserContext>

Setting `c8y.bridge.topic_prefix` will change the MQTT topics that the
Cumulocity bridge publishes to/listens to in mosquitto. The default value is
`c8y`, so the mappper publishes measurements to `c8y/s/us`, and this is
forwarded to Cumulocity on the `s/us` topic. In the example above, we set the
topic prefix to `c8y-second`, so the equivalent local topic would
`c8y-second/s/us`. It is recommended, but not required, to include `c8y` in the
topic prefix, to make it clear that the relevant topics are bridge topics that
will forward to and from Cumulocity.

### Cumulocity proxy bind port
<UserContext>
```
sudo tedge config set c8y.proxy.bind.port 8002 --profile $C8Y_PROFILE_NAME
```
</UserContext>

Since the Cumulocity mapper hosts a [proxy server for
Cumulocity](../../references/cumulocity-proxy.md) and there will be a second
mapper instance running, this configuration also needs to be unique per profile.

### Optional per-profile configurations
All the Cumulocity-specific configurations (those listed by `tedge config list
--doc c8y`) can be specified per-profile to match tenant-specific constraints.

## Connecting
Once the second cloud profile has been configured, you can finally connect the
second mapper using:

<UserContext>
```
sudo tedge connect c8y --profile $C8Y_PROFILE_NAME
```
</UserContext>

<UserContext language="" title="Output">
```
Connecting to Cumulocity with config:
        device id: $DEVICE_ID
        cloud profile: $C8Y_PROFILE_NAME
        cloud host: $C8Y_PROFILE_URL:8883
        certificate file: /etc/tedge/device-certs/tedge-certificate.pem
        bridge: mosquitto
        service manager: systemd
        mosquitto version: 2.0.11
Creating device in Cumulocity cloud... ✓
Restarting mosquitto... ✓
Waiting for mosquitto to be listening for connections... ✓
Verifying device is connected to cloud... ✓
Enabling tedge-mapper-c8y@$C8Y_PROFILE_NAME... ✓
Checking Cumulocity is connected to intended tenant... ✓
Enabling tedge-agent... ✓
```
</UserContext>

Once the mapper is running, you can restart it by running:

<UserContext>
```
sudo systemctl restart tedge-mapper-c8y@$C8Y_PROFILE_NAME
```
</UserContext>

This uses a systemd service template to create the `tedge-mapper-c8y@second`
service. If you are not using systemd, you will need to create a service
definition for `tedge-mapper-c8y@second` before attempting to connect your
device to a second Cumulocity instance.

## Environment variables
For easy configuration of profiles in shell scripts, you can set the profile
name using the environment variable `TEDGE_CLOUD_PROFILE`.

<UserContext language="sh" title="With arguments">
```
sudo tedge config set c8y.url $C8Y_URL
sudo tedge connect c8y

sudo tedge config set c8y.url $C8Y_PROFILE_URL --profile $C8Y_PROFILE_NAME
sudo tedge config set c8y.bridge.topic_prefix c8y-$C8Y_PROFILE_NAME --profile $C8Y_PROFILE_NAME
sudo tedge config set c8y.proxy.bind.port 8002 --profile $C8Y_PROFILE_NAME
sudo tedge connect c8y --profile $C8Y_PROFILE_NAME
```
</UserContext>

<UserContext language="sh" title="With environment variable">
```sh title="With environment variable"
# You can set the profile name to an empty string to use the default profile
export TEDGE_CLOUD_PROFILE=
sudo tedge config set c8y.url $C8Y_URL
sudo tedge connect c8y

export TEDGE_CLOUD_PROFILE=$C8Y_PROFILE_NAME
sudo tedge config set c8y.url $C8Y_PROFILE_URL
sudo tedge config set c8y.bridge.topic_prefix c8y-$C8Y_PROFILE_NAME
sudo tedge config set c8y.proxy.bind.port 8002
sudo tedge connect c8y

export TEDGE_CLOUD_PROFILE=
sudo tedge config get c8y.url #=> $C8Y_URL

export TEDGE_CLOUD_PROFILE=$C8Y_PROFILE_NAME
sudo tedge config get c8y.url #=> $C8Y_PROFILE_URL
```
</UserContext>

If you need to temporarily override a profiled configuration, you can use
environment variables of the form `TEDGE_C8Y_PROFILES_<NAME>_<CONFIGURATION>`.
For example:

<UserContext>
```
$ TEDGE_C8Y_PROFILES_$C8Y_PROFILE_NAME_URL=different.example.com tedge config get c8y.url --profile $C8Y_PROFILE_NAME
different.example.com
$ TEDGE_C8Y_PROFILES_$C8Y_PROFILE_NAME_PROXY_BIND_PORT=1234 tedge config get c8y.proxy.bind.port --profile $C8Y_PROFILE_NAME
1234
```
</UserContext>

If you are configuring %%te%% entirely with environment variables, e.g. in a
containerised deployment, you probably don't need to make use of cloud profiles
as you can set the relevant configurations directly on each mapper instance.