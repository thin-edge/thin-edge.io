---
title: Token
tags: [Operate, Cumulocity, JWT]
sidebar_position: 11
---

## How to retrieve a JWT (JSON Web Token) to authenticate on Cumulocity

## Overview

In order to [authenticate HTTP requests on Cumulocity](https://cumulocity.com/guides/10.5.0/reference/rest-implementation/#authentication),
a device can retrieve a token using MQTT.

## Retrieving the token

Follow the below steps in order to retrieve the token from the Cumulocity cloud using MQTT.

1. Subscribe to token topic

    ```sh te2mqtt
    tedge mqtt sub c8y/s/dat --no-topic
    ```

2. Publish an empty message on `c8y/s/uat` topic

    ```sh te2mqtt
    tedge mqtt pub c8y/s/uat ''
    ```

3. After a while the token will be published on the subscribed topic `c8y/s/dat` in the below format

    ```sh
    71,${Base64 encoded JWT}
    ```
