---
title: Cumulocity Token
tags: [Operate, Security, Cumulocity, JWT]
description: Requesting a token for manual Cumulocity API requests
---

:::tip
%%te%% provides an alternative way to access the Cumulocity REST API without having to request a token. The [Cumulocity Proxy Service](../../references/cumulocity-proxy.md) can be used which handles the authorization as required.

It is recommended to use the [Cumulocity Proxy Service](../../references/cumulocity-proxy.md) where possible.
:::

## Overview

For instances where you cannot use the [Cumulocity Proxy Service](../../references/cumulocity-proxy.md), the following instructions detail how to manually request a token, and then how to use the token to make manual REST API calls to the Cumulocity tenant.

## Retrieving the token

Follow the below steps in order to retrieve the token from Cumulocity using MQTT.

1. Subscribe to the token topic

    ```sh te2mqtt formats=v1
    tedge mqtt sub c8y/s/dat --no-topic
    ```

2. Publish an empty message on the `c8y/s/uat` topic

    ```sh te2mqtt formats=v1
    tedge mqtt pub c8y/s/uat ''
    ```

3. After a while the token will be published on the subscribed topic `c8y/s/dat` in the below format

    ```sh
    71,${Base64 encoded JWT}
    ```

    Store the token as required (e.g. assign to a variable), and use in REST API request to the Cumulocity tenant.

    :::note
    Typically tokens are only valid for 1 hour (depending on your Cumulocity settings), so you will need to request a new token before it expires otherwise your API Requests will return an Unauthorized (401) error.

    The expiration timestamp, issuer (Cumulocity URL), and tenant are encoded in the token and can be decoded using the standard [JSON Web Token Standard](https://datatracker.ietf.org/doc/html/rfc7519) which there should be a library in most popular programming languages.
    :::

### Alternative: Retrieving the token using mosquitto_rr

`mosquitto_rr` provides a simple cli interface to handle the request/response pattern, and can be used to create a simple one-liner to retrieve a new token.

The `mosquitto_rr` cli command can be installed on your operating system via one of the following packages:

```sh tab={"label":"Debian/Ubuntu"}
mosquitto-clients
```

```sh tab={"label":"RHEL/Fedora/RockyLinux"}
mosquitto
```

```sh tab={"label":"openSUSE"}
mosquitto-clients
```

```sh tab={"label":"Alpine"}
mosquitto
```

The token can be retrieved using the following one-liner:

```sh
export C8Y_TOKEN=$(mosquitto_rr -t c8y/s/uat -e c8y/s/dat -m '' | cut -d, -f2-)
```

Where:
* `-t` represents the request topic, where the `-m ''` message is sent to it
* `-e` represents the response topic which will print the message on the console


## Using token in REST API calls to Cumulocity

The retrieved token can be used to make HTTP calls to the [Cumulocity REST API](https://cumulocity.com/api/core/).

For simplicity, this example will retrieve the Cumulocity URL via the tedge cli command. If you are using a higher level programming language like python3, then you get the Cumulocity URL by decoding the token, e.g. using [PyJWT](https://pyjwt.readthedocs.io/en/latest/).

The following code snippet does the following steps:

1. Get the Cumulocity URL (using `tedge`)
2. Get the token (using `mosquitto_rr`)
3. Send a request (using `curl`)

```sh
# Get the Cumulocity URL
export C8Y_URL="https://$(tedge config get c8y.url)"

# Get the token (using mosquitto_rr cli command)
export C8Y_TOKEN=$(mosquitto_rr -t c8y/s/uat -e c8y/s/dat -m '' | cut -d, -f2-)

# Send a REST API call to Cumulocity
curl -s -H "Authorization: Bearer $C8Y_TOKEN" \
    -H "Accept: application/json" \
    "$C8Y_URL/user/currentUser"
```

:::tip
The same functionality (as above) can be achieved by using the [Cumulocity Proxy Service](../../references/cumulocity-proxy.md):

```sh
curl -s -H "Accept: application/json" \
    "http://localhost:8001/c8y/user/currentUser"
```
:::

```text title="Output (pretty printed)"
{
    "shouldResetPassword": false,
    "userName": "device_rpi4-d83add90fe56",
    "self": "https://t123456.eu-latest.cumulocity.com/user/currentUser",
    "effectiveRoles": [
        {
            "name": "ROLE_IDENTITY_ADMIN",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_IDENTITY_ADMIN",
            "id": "ROLE_IDENTITY_ADMIN"
        },
        {
            "name": "ROLE_EVENT_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_EVENT_READ",
            "id": "ROLE_EVENT_READ"
        },
        {
            "name": "ROLE_IDENTITY_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_IDENTITY_READ",
            "id": "ROLE_IDENTITY_READ"
        },
        {
            "name": "ROLE_INVENTORY_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_INVENTORY_READ",
            "id": "ROLE_INVENTORY_READ"
        },
        {
            "name": "ROLE_DEVICE_CONTROL_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_DEVICE_CONTROL_READ",
            "id": "ROLE_DEVICE_CONTROL_READ"
        },
        {
            "name": "ROLE_MEASUREMENT_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_MEASUREMENT_READ",
            "id": "ROLE_MEASUREMENT_READ"
        },
        {
            "name": "ROLE_AUDIT_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_AUDIT_READ",
            "id": "ROLE_AUDIT_READ"
        },
        {
            "name": "ROLE_USER_MANAGEMENT_OWN_ADMIN",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_USER_MANAGEMENT_OWN_ADMIN",
            "id": "ROLE_USER_MANAGEMENT_OWN_ADMIN"
        },
        {
            "name": "ROLE_ALARM_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_ALARM_READ",
            "id": "ROLE_ALARM_READ"
        },
        {
            "name": "ROLE_DEVICE",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_DEVICE",
            "id": "ROLE_DEVICE"
        },
        {
            "name": "ROLE_USER_MANAGEMENT_OWN_READ",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_USER_MANAGEMENT_OWN_READ",
            "id": "ROLE_USER_MANAGEMENT_OWN_READ"
        },
        {
            "name": "ROLE_INVENTORY_CREATE",
            "self": "https://t123456.eu-latest.cumulocity.com/user/roles/ROLE_INVENTORY_CREATE",
            "id": "ROLE_INVENTORY_CREATE"
        }
    ],
    "id": "device_rpi4-d83add90fe56",
    "lastPasswordChange": "2024-01-14T19:56:05.978Z"
}
```
