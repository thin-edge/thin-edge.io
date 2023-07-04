---
title: Connecting to AWS IoT
tags: [Getting Started, AWS, Connection]
sidebar_position: 4
---

# Connect your device to AWS IoT

The very first step to enable **thin-edge.io** is to connect your device to the cloud.

* This is a 10 minutes operation to be done only once.
* It establishes a permanent connection from your device to the cloud end-point.
* This connection is secure (encrypted over TLS), and the two peers are identified by x509 certificates.
* Sending data to the cloud will then be as simple as sending data locally.

The focus is here on connecting the device to AWS IoT.
See this [tutorial](connect-c8y.md), if you want to connect Cumulocity IoT instead.
See this [tutorial](connect-azure.md), if you want to connect Azure IoT instead.

Before you try to connect your device to AWS IoT, you need:

* [Install `thin-edge.io` on your device](../install/index.md).

You can now use [`tedge` command](../references/cli/index.md) to:

* [create a certificate for your device](connect-aws.md#create-the-certificate),
* [register the device on AWS IoT Hub](connect-aws.md#register-the-device-on-AWS),
* [configure the device](connect-aws.md#configure-the-device),
* [connect the device](connect-aws.md#connect-the-device), and
* [send your first telemetry data](#sending-your-first-telemetry-data).

## Create the certificate

The `tedge cert create` command creates a self-signed certificate which can be used for testing purpose.

A single argument is required: an identifier for the device.
This identifier will be used to uniquely identify your devices among others in your cloud tenant.
This identifier will be also used as the Common Name (CN) of the certificate.
Indeed, this certificate aims to authenticate that this device is the device with that identity.

```sh
sudo tedge cert create --device-id my-device
```

## Show certificate details

You can then check the content of that certificate.

```sh
sudo tedge cert show
```

```text title="Output"
Device certificate: /etc/tedge/device-certs/tedge-certificate.pem
Subject: CN=my-device, O=Thin Edge, OU=Test Device
Issuer: CN=my-device, O=Thin Edge, OU=Test Device
Valid from: Tue, 09 Mar 2021 14:10:30 +0000
Valid up to: Thu, 10 Mar 2022 14:10:30 +0000
Thumbprint: 860218AD0A996004449521E2713C28F67B5EA580
```

You may notice that the issuer of this certificate is the device itself.
This is a self-signed certificate.
To use a certificate signed by your Certificate Authority,
see the reference guide of [`tedge cert`](../references/cli/tedge-cert.md).

## Register the device on AWS IoT Hub

For a device to be trusted by AWS, one needs a device certificate and the `tedge cert create` command is the simplest way to get one.
Also a policy needs to be attached to the device certificate in AWS IoT Core. AWS IoT Core policies determine what an
authenticated identity can do (here the authenticated identity is the device being connected). More info on AWS IoT Core
policies can be found [here](https://docs.aws.amazon.com/iot/latest/developerguide/iot-policies.html).

To create a new policy, head over to the AWS IoT Core and navigate to

**Security &rarr; Policies &rarr; Create policy &rarr; Policy properties &rarr; Policy name &rarr; Enter the name of your policy (e.g.tedge)**

On the Policy statements tab click on ***JSON*** and enter the policy in the ***Policy document*** (an example policy can be found [here](./aws-example-policy.json)) then click ***Create***.

In the AWS IoT Core, navigate to ***Manage &rarr; All devices &rarr; Things &rarr; Create things &rarr; Create Single thing &rarr; Next***. Enter the Thing name which can be obtained from the device with the output of the following command:

```sh
tedge config get device.id
```

In the Device Shadow section which allows connected devices to sync states with AWS choose ***Unnamed shadow (classic)*** and click ***Next*** and ***Configure device certificate - optional*** page opens.

At ***Device certificate*** choose ***Use my certificate &rarr; CA is not registered with AWS IoT*** then ***Choose file*** and select your tedge-certificate.pem file, click on ***Open &rarr; Next***.

The last step needed is to attach previously created policy to your certificate, **Attach policies to certificate -optional** &rarr; Select your created policy **&rarr; Create thing**.

## Configure the device

To connect the device to the AWS IoT Hub, one needs to set the URL of the IoT Hub and the root certificate of the IoT
Hub as below.

Set the URL of your AWS IoT Hub.

```sh
sudo tedge config set aws.url "${AWS_URL}"
```

```sh title="Example"
sudo tedge config set aws.url "a2e8ahbpo21syc.iot.eu-central-1.amazonaws.com"
```

The URL is unique to the AWS account and region that is used, and can be found in the AWS IoT Core by navigating to
"Settings". It will be listed under "Device data endpoint" (e.g. `a2e8ahbpo21syc.iot.eu-central-1.amazonaws.com`)

Set the path to the root certificate if necessary. The default is `/etc/ssl/certs`.

```sh
sudo tedge config set aws.root_cert_path /etc/ssl/certs/AmazonRootCA1.pem
```

This will set the root certificate path of the AWS IoT Hub. In most of the Linux flavors, the certificate will be
present in /etc/ssl/certs. If not found download it from
[here](https://docs.aws.amazon.com/iot/latest/developerguide/server-authentication.html#server-authentication-certs).

## Connect the device

Now, you are ready to get your device connected to AWS IoT Hub with `tedge connect aws`.
This command configures the MQTT broker:

* to establish a permanent and secure connection to the AWS cloud,
* to forward local messages to the cloud and vice versa.

Also, if you have installed `tedge-mapper`, this command starts and enables the tedge-mapper-aws systemd service.
At last, it sends packets to AWS IoT Hub to check the connection.

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

## Sending your first telemetry data

Using the AWS mapper, you can publish measurement telemetry data to AWS by publishing on the `tedge/measurements` topic:

```sh te2mqtt
tedge mqtt pub tedge/measurements '{"temperature": 21.3}'
```

Alternatively, post your own custom messages on `aws/td/#` topic:

```sh te2mqtt
tedge mqtt pub aws/td '{"text": "My message"}'
```

To view the messages that were sent from the device to the cloud, follow this
[document](https://docs.aws.amazon.com/iot/latest/developerguide/view-mqtt-messages.html).

## Next Steps

You can now:

* learn how to [send various kind of telemetry data](send-thin-edge-data.md)
  using the cloud-agnostic [Thin-Edge-Json data format](../understand/thin-edge-json.md),
* or have a detailed view of the [topics mapped to and from AWS](../references/mqtt-topics.md#aws-mqtt-topics)
  if you prefer to use directly AWS specific formats and protocols.
