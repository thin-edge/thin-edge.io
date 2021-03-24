# Connect your device to Azure IoT

The very first step to enable `thin-edge.io` is to connect your device to the cloud.
* This is a 10 minutes operation to be done only once.
* It establishes a permanent connection from your device to the cloud end-point.
* This connection is secure (encrypted over TLS), and the two peers are identified by x509 certificates.
* Sending data to the cloud will then be as simple as sending data locally.

The focus is here on connecting the device to [Azure IoT Hub](https://azure.microsoft.com/en-in/services/iot-hub/).
See this [tutorial](connect-c8y.md), if you want to connect Cumulocity IoT instead.

Before you try to connect your device to Azure IoT, you need:
* The url of the Azure iothub endpoint to connect (e.g. `[iot-hub-name].azure-devices.net`).
* Your credentials to connect Azure:
    * Your user name and password.
    * None of these credentials will be stored on the device.
    * These are only required once, to register the device.

If not done yet, [install `thin-edge.io` on your device](../howto-guides/002_installation.md).

You can now to use [`tegde` command](../references/tedge.md) to:
* [create a certificate for you device](connect-azure.md#create-the-certificate),
* [make the device certificate trusted by Azure](connect-azure.md#make-the-device-trusted-by-azure),
* [connect the device](connect-azure.md#connect-the-device), and
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

## Show certificate details

You can then check the content of that certificate.

```
$ tedge cert show
Device certificate: /home/ubuntu/.tedge/tedge-certificate.pem
Subject: CN=my-tbr, O=Thin Edge, OU=Test Device
Issuer: CN=my-tbr, O=Thin Edge, OU=Test Device
Valid from: Tue, 09 Mar 2021 14:10:30 +0000
Valid up to: Thu, 10 Mar 2022 14:10:30 +0000
Thumbprint: 860218AD0A996004449521E2713C28F67B5EA580

```

You may notice that the issuer of this certificate is the device itself.
This is a self-signed certificate.
The Thumbprint is the Sha1sum of the certificate. This is required for registering the
device using the self-signed certificate on Azure.
To use a certificate signed by your Certificate Authority,
see the reference guide of [`tedge cert`](../references/tedge-cert.md).

## Register the device on Azure

For a device to be trusted by Azure, one needs to add the self-signed certificate thumbprint on to the Azure iothub portal.
In the Azure iothub portal, navigate to "Explores/IoT Devices click on + New", this will open a new blade "Create a device".

Here provide the configuration parameters that are required to create the device as described below
   * Device ID: Should be same as the Subject of the certificate
   * Authentication type: Select "X.509 Self-Signed" option
      * Provide the Primary Thumbprint that was displayed in [`tedge cert show`](connect-azure.md#Show certificate details)
      * Use the same for the Secondary Thumbprint as well (Since we are using a single certificate)
   * Set Connect this device to an IoT hub to "Enable"
   * Then save the configuration
Upon successfully saved the configuration a new device has been created on the IoT hub.
The new device can be seen on the iot hub portal by navigating to "Explores/IoT Devices".

## Connect the device

Now, you are ready to run `tedge connect az`.
This command configures the MQTT broker:
* to establish a permanent and secure connection to the Azure cloud,
* to forward local messages to the cloud and vice versa.

```
$ target/release/tedge connect az

Checking if systemd and mosquitto are available.

Checking if configuration for requested bridge already exists.

Validate the bridge certificates.

Saving configuration for requested bridge.

Restarting mosquitto, [requires elevated permission], authorise when asked.

Awaiting mosquitto to start. This may take up to 5 seconds.

Persisting mosquitto on reboot.

Successfully created bridge connection!

Sending packets to check connection. This may take up to 10 seconds.

Received expected response message, connection check is successful.
```

## Sending your first telemetry data

Sending data to Azure is done using [MQTT](../architecture/mqtt-bus.md) over topics prefixed with `az`.
Any messages sent on the topic will be forwarded to Azure.
Here, we use `tedge mqtt pub az/messages/events/` a message to be understood as a temperature of 20 Celsius.

```
$ tedge mqtt pub az/messages/events {"temperature": 20}
```

More info about sending telemetry to Azure can be found [Here](https://docs.microsoft.com/en-us/azure/iot-hub/quickstart-send-telemetry-dotnet)

## Next Steps

You can now:
* learn how to [send various kind of telemetry data](send-thin-edge-data.md)
  using the cloud-agnostic [Thin-Edge-Json data format](../architecture/thin-edge-json.md),
* or have a detailed view of the [topics mapped to and from Azure](../references/tedge-mapper.md)
  if you prefer to use directly Azure specific formats and protocols.
