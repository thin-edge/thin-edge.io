# Connect your device to Cumulocity IoT

The very first step to enable `thin-edge` is to connect your device to the cloud.
* This is a 10 minutes operation to be done only once.
* It establishes a permanent connection from your device to the cloud end-point.
* This connection is secure (encrypted over TLS), and the two peers are identified by x509 certificates.
* Sending data the cloud will be then as simple as sending data locally.

The focus is here on connecting to [Cumulocity IoT](https://www.cumulocity.com/guides/concepts/introduction/).
See this [tutorial](connect-azure.md), if you want to connect Azure IoT instead.

Before you try to connect your device to Cumulocity IoT, you need:
* The url of the endpoint to connect (e.g. `eu-latest.cumulocity.com`).
* Your credentials to connect Cumulocity:
    * Your tenant identifier (e.g. `t00000007`), a user name and password.
    * None of these credentials will be stored on the device.
    * These are only required once, to register the device.

If not done yet, [install `thin-edge` on your device](../howto-guides/002_installation.md).

You can now to use [`tegde` command](../references/tedge.md) to:
* [create a certificate for you device](connect-c8y.md#create-the-certificate),
* [make the device certificate trusted by Cumulocity](connect-c8y.md#make-the-device-trusted-by-cumulocity),
* [connect the device](connect-c8y.md#connect-the-device), and
* [send your first telemetry data](#sending-your-first-telemetry-data).

## Create the certificate

The `tedge cert create` command creates a self-signed certificate which can be used for testing purpose.

A single argument is required: an identifier for the device.
This identifier will be used to uniquely identify your devices among others in your cloud tenant.
This identifier will be also used as the Common Name (CN) of the certificate.
Indeed, this certificate aims to authenticate that this device is actually the device with that identity. 

```
$ tedge cert create --id my-device
```

You can then check the content of that certificate.

```
$ tedge cert show
Device certificate: /home/pi/.tedge/tedge-certificate.pem
Subject: CN=my-device, O=Thin Edge, OU=Test Device
Issuer: CN=my-device, O=Thin Edge, OU=Test Device
Valid from: Tue, 09 Feb 2021 17:16:52 +0000
Valid up to: Tue, 11 May 2021 17:16:52 +0000
```

You may notice that the issuer of this certificate is the device itself.
This is a self-signed certificate.
To use a certificate signed by your Certificate Authority,
see the reference guide of [`tedge cert`](../references/tedge-cert.md).

## Make the device trusted by Cumulocity

For a certificate to be trusted by Cumulocity,
one needs to add the certificate of the signing authority to the list of trusted certificates.
In the Cumulocity GUI, navigate to "Device Management/Management/Trusted certificates"
in order to see this list for your Cumulocity tenant.

Here, the device certificate is self-signed and has to be directly trusted by Certificate.
This can be done:
* either with the GUI: upload the certificate from your device (`/home/pi/.tedge/tedge-certificate.pem`)
  to your tenant "Device Management/Management/Trusted certificates".
* or using the `tedge cert register c8y` command.

```
$ tedge cert register c8y \
  --url <your instance url> \
  --tenant <your tenant id>\
  --user <your user name>
```

## Connect the device

Now, you are ready to run `tedge connect c8y`.
This command configures the MQTT broker:
* to establish a permanent and secure connection to the cloud,
* to forward local messages to the cloud and vice versa.

```
$ target/release/tedge connect c8y --id my-device
Checking if systemd and mosquitto are available.

Checking if configuration for requested bridge already exists.

Checking configuration for requested bridge.

Creating configuration for requested bridge.

Restarting MQTT Server, [requires elevated permission], please authorise if asked.

Awaiting MQTT Server to start. This may take few seconds.

Sending packets to check connection.
Persisting MQTT Server on reboot.

Successully created bridge connection!
```

## Sending your first telemetry data

Sending data to Cumulocity is done using [MQTT](../architecture/mqtt-bus.md) over topics prefixed with `c8y`.
Any messages sent to one of these topics will be forwarded to Cumulocity.
The messages are expected to have a format specific to each topic.
Here, we use `tedge mqtt pub` a raw Cumulocity SmartRest message to be understood as a temperature of 20 Celsius.

```
$ tedge mqtt pub c8y/s/us 211,20
```

To check that this message has been received by Cumulocity,
navigate to "Device Management/Devices/All devices/<your device id>/Measurements".
You should observe a "temperature measurement" graph with the new data point.


## Next Steps

You can now:
* learn how to [send various kind of telemetry data](send-thin-edge-data.md)
  using the cloud-agnostic [Thin-Edge-Json data format](../architecture/thin-edge-json.md),
* or have a detailed view of the [topics mapped to and from Cumulocity](../references/tedge-mapper.md)
  if you prefer to use directly Cumulocity specific formats and protocols.
