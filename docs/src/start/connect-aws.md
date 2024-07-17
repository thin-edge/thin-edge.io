---
title: Connecting to AWS IoT
tags: [Getting Started, AWS, Connection]
sidebar_position: 4
description: Connect %%te%% to AWS IoT and publish telemetry data
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing relevant settings which will be reflected in the instructions. It makes it even easier to explore and use %%te%%.

<UserContextForm settings="DEVICE_ID,AWS_URL,AWS_REGION,AWS_ACCOUNT_ID" />

The user context will be persisted in your web browser's local storage.
:::

### Overview

The very first step to enable %%te%% is to connect your device to the cloud.

* This is a 10 minutes operation to be done only once.
* It establishes a permanent connection from your device to the cloud end-point.
* This connection is secure (encrypted over TLS), and the two peers are identified by x509 certificates.
* Sending data to the cloud will then be as simple as sending data locally.

The focus is here on connecting the device to AWS IoT.
See this [tutorial](connect-c8y.md), if you want to connect Cumulocity IoT instead.
See this [tutorial](connect-azure.md), if you want to connect Azure IoT instead.

Before you try to connect your device to AWS IoT, you need:

* [Install %%te%% on your device](../install/index.md).

You can now use [`tedge` command](../references/cli/index.md) to:

* [create a certificate for your device](#create-certificate),
* [register the device on AWS IoT Core](#register),
* [configure the device](#configure),
* [connect the device](#connect), and
* [send your first telemetry data](#send).

## Create the certificate {#create-certificate}

The `tedge cert create` command creates a self-signed certificate which can be used for testing purpose.

A single argument is required: an identifier for the device.
This identifier will be used to uniquely identify your devices among others in your cloud tenant.
This identifier will be also used as the Common Name (CN) of the certificate.
Indeed, this certificate aims to authenticate that this device is the device with that identity.


<UserContext>

```sh
sudo tedge cert create --device-id $DEVICE_ID
```

</UserContext>

## Show certificate details

You can then check the content of that certificate.

```sh
sudo tedge cert show
```

<UserContext>

```text title="Output"
Device certificate: /etc/tedge/device-certs/tedge-certificate.pem
Subject: CN=$DEVICE_ID, O=Thin Edge, OU=Test Device
Issuer: CN=$DEVICE_ID, O=Thin Edge, OU=Test Device
Valid from: Tue, 09 Mar 2021 14:10:30 +0000
Valid up to: Thu, 10 Mar 2022 14:10:30 +0000
Thumbprint: 860218AD0A996004449521E2713C28F67B5EA580
```

</UserContext>

You may notice that the issuer of this certificate is the device itself.
This is a self-signed certificate.
To use a certificate signed by your Certificate Authority,
see the reference guide of [`tedge cert`](../references/cli/tedge-cert.md).

## Register the device on AWS IoT Core {#register}

For a device to be trusted by AWS, one needs a device certificate and the `tedge cert create` command is the simplest way to get one.
Also a policy needs to be attached to the device certificate in AWS IoT Core. AWS IoT Core policies determine what an
authenticated identity can do (here the authenticated identity is the device being connected). More info on AWS IoT Core
policies can be found [here](https://docs.aws.amazon.com/iot/latest/developerguide/iot-policies.html).

### Create new policy

Before you can register any devices, you must create a policy which can be assigned to each new device during the registration process. The policy grants permissions to the devices to be able to write to specific MQTT topics.

1. Open your AWS IoT Core

2. Navigate to the Policies section under **Security** &rarr; **Policies**

3. Click **Create policy**, and enter a policy name (e.g. `thin-edge.io`)

4. Under the **Policy statements** tab click on ***JSON*** and add the following JSON:

    :::tip
    If you haven't provided the [User Context](#user-context) then it is strongly encouraged to do so, as the following snippet will be updated to reflect your AWS IoT Core settings.
    :::

    <UserContext>

    ```json
    {
      "Version": "2012-10-17",
      "Statement": [
        {
          "Effect": "Allow",
          "Action": "iot:Connect",
          "Resource": "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:client/${iot:Connection.Thing.ThingName}"
        },
        {
          "Effect": "Allow",
          "Action": "iot:Subscribe",
          "Resource": [
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topicfilter/thinedge/${iot:Connection.Thing.ThingName}/cmd/#",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topicfilter/$aws/things/${iot:Connection.Thing.ThingName}/shadow/#",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topicfilter/thinedge/devices/${iot:Connection.Thing.ThingName}/test-connection"
          ]
        },
        {
          "Effect": "Allow",
          "Action": "iot:Receive",
          "Resource": [
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/thinedge/${iot:Connection.Thing.ThingName}/cmd",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/thinedge/${iot:Connection.Thing.ThingName}/cmd/*",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/$aws/things/${iot:Connection.Thing.ThingName}/shadow",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/$aws/things/${iot:Connection.Thing.ThingName}/shadow/*",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/thinedge/devices/${iot:Connection.Thing.ThingName}/test-connection"
          ]
        },
        {
          "Effect": "Allow",
          "Action": "iot:Publish",
          "Resource": [
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/thinedge/${iot:Connection.Thing.ThingName}/td",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/thinedge/${iot:Connection.Thing.ThingName}/td/*",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/$aws/things/${iot:Connection.Thing.ThingName}/shadow",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/$aws/things/${iot:Connection.Thing.ThingName}/shadow/*",
            "arn:aws:iot:$AWS_REGION:$AWS_ACCOUNT_ID:topic/thinedge/devices/${iot:Connection.Thing.ThingName}/test-connection"
          ]
        }
      ]
    }
    ```

    </UserContext>

    :::info
    A static form of the example policy can be downloaded from the [repository](https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/docs/src/start/aws-example-policy.json), though you will need to replace the `<region>` and `<account-id>` placeholders with your AWS IoT Core values.
    :::

4. Click **Create**

### Registering a Device (thing)

1. Open the AWS IoT Core page

2. Navigate to **Manage** &rarr; **All devices** &rarr; **Things**

3. Click **Create things**, select the **Create single thing** option, then click **Next**

4. Enter the device name (e.g. Thing name)

    <UserContext>

    ```
    $DEVICE_ID
    ```

    </UserContext>

    :::tip
    The thing name must match the device certificate's Common Name. If you are unsure what the value is, then you can check the common name of the device by using the following `tedge` command:

    ```sh
    tedge config get device.id
    ```
    :::

4. Select the ***Unnamed shadow (classic)*** option and click ***Next***

5. On the ***Device certificate*** page, choose ***Use my certificate &rarr; CA is not registered with AWS IoT***

6. ***Choose file*** and select your `tedge-certificate.pem` file, click on ***Open &rarr; Next***

    If you haven't previously downloaded the public device certificate, you can print the contents (don't worry this is not sensitive information), and then you can save the certificate's contents to a file on your computer so it can be uploaded to AWS.

    You can print the public certificate to the console using the following command:

    ```sh
    cat /etc/tedge/device-certs/tedge-certificate.pem
    ```

7. On the **Policies** page, select the previously created policy, e.g. `thin-edge.io`, then click **Create thing**

## Configure the device {#configure}

To connect the device to the AWS IoT Core, one needs to set the URL of the IoT Hub and the root certificate of the IoT
Hub as below.

Set the URL of your AWS IoT Core.

<UserContext>

```sh
sudo tedge config set aws.url "$AWS_URL"
```

</UserContext>

The URL is unique to the AWS account and region that is used, and can be found in the AWS IoT Core by navigating to
**Settings**. It will be listed under **Device data endpoint**.


:::note
The Amazon's public root certificate for your region's AWS IoT Core needs to be present in your ca-certificate store for the communication to the cloud to be trusted.

In most of the Linux flavors, the certificate will be present in `/etc/ssl/certs` directory. If the certificate is not present, then you can download it manually from
[Amazon](https://docs.aws.amazon.com/iot/latest/developerguide/server-authentication.html#server-authentication-certs) and add it to your system's ca-certificate store by following the
[Adding a root certificate](../operate/security/cloud-authentication.md#adding-a-root-certificate) documentation.
:::

## Connect the device {#connect}

Now, you are ready to get your device connected to AWS IoT Core with `tedge connect aws`.
This command configures the MQTT broker:

* to establish a permanent and secure connection to the AWS cloud,
* to forward local messages to the cloud and vice versa.

Also, if you have installed `tedge-mapper`, this command starts and enables the tedge-mapper-aws systemd service.
At last, it sends packets to AWS IoT Core to check the connection.

```sh
sudo tedge connect aws
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Saving configuration for requested bridge.

Restarting mosquitto service.

Awaiting mosquitto to start. This may take up to 5 seconds.

Enabling mosquitto service on reboots.

Successfully created bridge connection!

Sending packets to check connection. This may take up to 2 seconds.

Received expected response on topic aws/connection-success, connection check is successful.
Connection check is successful.

Checking if tedge-mapper is installed.

Starting tedge-mapper-aws service.

Persisting tedge-mapper-aws on reboot.

tedge-mapper-aws service successfully started and enabled!

```

If your device does not have internet access and you want to create the bridge configuration, you can run a `tedge connect aws` with the `--offline` flag.

```sh
sudo tedge connect aws --offline
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Saving configuration for requested bridge.

Restarting mosquitto service.

Awaiting mosquitto to start. This may take up to 5 seconds.

Enabling mosquitto service on reboots.

Successfully created bridge connection!

Offline mode. Skipping connection check.

Checking if tedge-mapper is installed.

Starting tedge-mapper-aws service.

Persisting tedge-mapper-aws on reboot.

tedge-mapper-aws service successfully started and enabled!

```

:::tip
If you having troubles with the connection, then check the following items:

* Check that the policy is attached to the device's certificate
* Check that the policy includes your AWS IoT Core's region and account id
* Check [Amazon diagnosing connectivity issues](https://docs.aws.amazon.com/iot/latest/developerguide/diagnosing-connectivity-issues.html) documentation for further debugging steps
* Check the mosquitto logs for errors, e.g. "verify" errors would indicate that you are missing Amazon's root certificate in the ca-certificate store
:::

## Sending your first telemetry data {#send}

Using the AWS mapper, you can publish measurement telemetry data to AWS by publishing on the `te/device/main///m/` topic:

```sh te2mqtt formats=v1
tedge mqtt pub te/device/main///m/environment '{"temperature": 21.3}'
```

Alternatively, post your own custom messages on `aws/td/#` topic:

```sh te2mqtt formats=v1
tedge mqtt pub aws/td '{"text": "My message"}'
```

To view the messages that were sent from the device to the cloud, follow this
[document](https://docs.aws.amazon.com/iot/latest/developerguide/view-mqtt-messages.html).

## Next Steps

You can now:

* learn how to [send various kind of telemetry data](send-measurements.md)
  using the cloud-agnostic [%%te%% JSON data format](../understand/thin-edge-json.md),
* or have a detailed view of the [topics mapped to and from AWS](../references/mappers/mqtt-topics.md#aws-mqtt-topics)
  if you prefer to use directly AWS specific formats and protocols.
