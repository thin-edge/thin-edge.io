---
title: Connecting to Cumulocity IoT
tags: [Getting Started, Cumulocity, Connection]
sidebar_position: 2
---

# Connect your device to Cumulocity IoT

The very first step to enable `thin-edge.io` is to connect your device to the cloud.
* This is a 10 minutes operation to be done only once.
* It establishes a permanent connection from your device to the cloud end-point.
* This connection is secure (encrypted over TLS), and the two peers are identified by x509 certificates.
* Sending data to the cloud will then be as simple as sending data locally.

The focus is here on connecting to [Cumulocity IoT](https://www.cumulocity.com/guides/concepts/introduction/).
See this [tutorial](connect-azure.md), if you want to connect Azure IoT instead.
See this [tutorial](connect-aws.md), if you want to connect AWS IoT instead.

Before you try to connect your device to Cumulocity IoT, you need:
* The url of the endpoint to connect (e.g. `eu-latest.cumulocity.com`).
* Your credentials to connect Cumulocity:
    * Your tenant identifier (e.g. `t00000007`), a user name and password.
    * None of these credentials will be stored on the device.
    * These are only required once, to register the device.

If not done yet, [install `thin-edge.io` on your device](../install/index.md).

You can now use the [`tedge` command](../references/cli/index.md) to:
* [create a certificate for you device](connect-c8y.md#create-the-certificate),
* [make the device certificate trusted by Cumulocity](connect-c8y.md#make-the-device-trusted-by-cumulocity),
* [connect the device](connect-c8y.md#connect-the-device), and
* [send your first telemetry data](#sending-your-first-telemetry-data).

## Configure the device

To connect the device to the Cumulocity IoT, one needs to set the URL of your Cumulocity IoT tenant and the root certificate as below.

Set the URL of your Cumulocity IoT tenant.

```sh
sudo tedge config set c8y.url your-tenant.cumulocity.com
```

Set the path to the root certificate if necessary. The default is `/etc/ssl/certs`.

```sh
sudo tedge config set c8y.root_cert_path /etc/ssl/certs
```

This will set the root certificate path of the Cumulocity IoT.
In most of the Linux flavors, the certificate will be present in `/etc/ssl/certs`.
If not found download it from [here](https://www.identrust.com/dst-root-ca-x3).


## Connecting to Cumulocity server signed with self-signed certificate

If the Cumulocity IoT instance that you're connecting to, is signed with a self-signed certificate(eg: Cumulocity IoT Edge instance),
then the path to that server certificate must be set as the c8y.root_cert_path as follows:

```sh
sudo tedge config set c8y.root_cert_path /path/to/the/self-signed/certificate
```

:::info
This is the certificate chain of the server and not the device's certificate kept at `/etc/tedge/device-certs` directory.
:::

If the Cumulocity server's certificate chain file isn't available locally, it can be downloaded using a web browser or using some other
third-party tools like openssl command as follows (to be adjusted based on your env):

```sh
openssl s_client -connect <hostname>:<port> < /dev/null 2>/dev/null \
| sed -ne '/-BEGIN CERTIFICATE-/,/-END CERTIFICATE-/p'
```

## Create the certificate

The `tedge cert create` command creates a self-signed certificate which can be used for testing purpose.

A single argument is required: an identifier for the device.
This identifier will be used to uniquely identify your devices among others in your cloud tenant.
This identifier will be also used as the Common Name (CN) of the certificate.
Indeed, this certificate aims to authenticate that this device is actually the device with that identity.

```sh
sudo tedge cert create --device-id my-device
```

```text title="Output"
Certificate was successfully created
```

You can then check the content of that certificate.

```sh
sudo tedge cert show
```

```text title="Output"
Device certificate: /etc/tedge/device-certs/tedge-certificate.pem
Subject: CN=my-device, O=Thin Edge, OU=Test Device
Issuer: CN=my-device, O=Thin Edge, OU=Test Device
Valid from: Tue, 09 Feb 2021 17:16:52 +0000
Valid up to: Tue, 11 May 2021 17:16:52 +0000
Thumbprint: CDBF4EC17AA02829CAC4E4C86ABB82B0FE423D3E
```

You may notice that the issuer of this certificate is the device itself.
This is a self-signed certificate.
To use a certificate signed by your Certificate Authority,
see the reference guide of [`tedge cert`](../references/cli/tedge-cert.md).

## Make the device trusted by Cumulocity

For a certificate to be trusted by Cumulocity,
one needs to add the certificate of the signing authority to the list of trusted certificates.
In the Cumulocity GUI, navigate to "Device Management/Management/Trusted certificates"
in order to see this list for your Cumulocity tenant.

Here, the device certificate is self-signed and has to be directly trusted by Certificate.
This can be done:
* either with the GUI: upload the certificate from your device (`/etc/tedge/device-certs/tedge-certificate.pem`)
  to your tenant "Device Management/Management/Trusted certificates".
* or using the `tedge cert upload c8y` command.

```sh
sudo tedge cert upload c8y --user "${C8Y_USER}"
```

```sh title="Example"
sudo tedge cert upload c8y --user "john.smith@example.com"
```

:::tip
To upload the certificate to cumulocity this user needs to have "Tenant management" admin rights.
If you get an error 503 here, check the appropriate rights in cumulocity user management.
:::

## Connect the device

Now, you are ready to run `tedge connect c8y`.
This command configures the MQTT broker:
* to establish a permanent and secure connection to the cloud,
* to forward local messages to the cloud and vice versa.

Also, if you have installed `tedge-mapper`, this command starts and enables the tedge-mapper-c8y systemd service.
At last, it sends packets to Cumulocity to check the connection.
If your device is not yet registered, you will find the digital-twin created in your tenant after `tedge connect c8y`!

```sh
sudo tedge connect c8y
```

```text title="Output"
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Creating the device in Cumulocity cloud.

Saving configuration for requested bridge.

Restarting mosquitto service.

Awaiting mosquitto to start. This may take up to 5 seconds.

Enabling mosquitto service on reboots.

Successfully created bridge connection!

Sending packets to check connection. This may take up to 2 seconds.

Connection check is successful.

Checking if tedge-mapper is installed.

Starting tedge-mapper-c8y service.

Persisting tedge-mapper-c8y on reboot.

tedge-mapper-c8y service successfully started and enabled!

Enabling software management.

Checking if tedge-agent is installed.

Starting tedge-agent service.

Persisting tedge-agent on reboot.

tedge-agent service successfully started and enabled!
```

## Sending your first telemetry data

Sending data to Cumulocity is done using MQTT over topics prefixed with `c8y`.
Any messages sent to one of these topics will be forwarded to Cumulocity.
The messages are expected to have a format specific to each topic.
Here, we use `tedge mqtt pub` a raw Cumulocity SmartRest message to be understood as a temperature of 20°C.

```sh te2mqtt
tedge mqtt pub c8y/s/us 211,20
```

To check that this message has been received by Cumulocity,
navigate to:

Device Management &rarr; Devices &rarr; All devices &rarr; `device_id` &rarr; Measurements

You should observe a "temperature measurement" graph with the new data point.

## Next Steps

You can now:
* learn how to [send various kind of telemetry data](send-thin-edge-data.md)
  using the cloud-agnostic [Thin-Edge-Json data format](../understand/thin-edge-json.md),
* or have a detailed view of the [topics mapped to and from Cumulocity](../references/mqtt-topics.md#cumulocity-mqtt-topics)
  if you prefer to use directly Cumulocity specific formats and protocols.
* learn how to [add custom fragments to cumulocity](../operate/c8y/c8y_fragments.md).
