# Connect your device to Azure IoT

The very first step to enable **thin-edge.io** is to connect your device to the cloud.
* This is a 10 minutes operation to be done only once.
* It establishes a permanent connection from your device to the cloud end-point.
* This connection is secure (encrypted over TLS), and the two peers are identified by x509 certificates.
* Sending data to the cloud will then be as simple as sending data locally.

The focus is here on connecting the device to Azure IoT.
See this [tutorial](connect-c8y.md), if you want to connect Cumulocity IoT instead.

Before you try to connect your device to Azure IoT, you need:
* Create a Azure **IoT Hub** in Azure portal as described [here](https://docs.microsoft.com/en-us/azure/iot-hub/iot-hub-create-through-portal).
* [Install `thin-edge.io` on your device](../howto-guides/002_installation.md).

You can now use [`tedge` command](../references/tedge.md) to:
* [create a certificate for your device](connect-azure.md#create-the-certificate),
* [register the device on Azure IoT Hub](connect-azure.md#register-the-device-on-Azure),
* [configure the device](connect-azure.md#configure-the-device),
* [connect the device](connect-azure.md#connect-the-device), and
* [send your first telemetry data](#sending-your-first-telemetry-data).

## Create the certificate

The `tedge cert create` command creates a self-signed certificate which can be used for testing purpose.

A single argument is required: an identifier for the device.
This identifier will be used to uniquely identify your devices among others in your cloud tenant.
This identifier will be also used as the Common Name (CN) of the certificate.
Indeed, this certificate aims to authenticate that this device is the device with that identity.

```
$ sudo tedge cert create --device-id my-device
```

## Show certificate details

You can then check the content of that certificate.

```
$ sudo tedge cert show
Device certificate: /etc/tedge/device-certs/tedge-certificate.pem
Subject: CN=my-device, O=Thin Edge, OU=Test Device
Issuer: CN=my-device, O=Thin Edge, OU=Test Device
Valid from: Tue, 09 Mar 2021 14:10:30 +0000
Valid up to: Thu, 10 Mar 2022 14:10:30 +0000
Thumbprint: 860218AD0A996004449521E2713C28F67B5EA580
```

You may notice that the issuer of this certificate is the device itself.
This is a self-signed certificate.
The Thumbprint is the Sha1sum of the certificate. This is required for registering the
device using the self-signed certificate on Azure IoT Hub.
To use a certificate signed by your Certificate Authority,
see the reference guide of [`tedge cert`](../references/tedge-cert.md).

## Register the device on Azure IoT Hub

For a device to be trusted by Azure, one needs to add the self-signed certificate thumbprint to the Azure IoT Hub Portal.
In the Azure IoT Hub Portal, navigate to "Explores"->"IoT Devices" click on "+ New", this will open a new blade "Create a device".

Here provide the configuration parameters that are required to create the device as described below.
   * Device ID: Should be the same as the Subject of the certificate.
   * Authentication type: Select **X.509 Self-Signed** option.
      * Provide the Primary Thumbprint that was displayed in [`tedge cert show`](connect-azure.md#show-certificate-details).
      * Use the same for the Secondary Thumbprint as well (Since we are using a single certificate).
   * Set "Connect this device to an IoT Hub" to **Enable**.
   * Then save the configuration.
Upon successfully saved the configuration a new device has been created on the IoT Hub.
The new device can be seen on the IoT Hub portal by navigating to "Explores"->"IoT Devices".

More info about registering a device can be found [here](https://docs.microsoft.com/en-us/azure/iot-edge/how-to-authenticate-downstream-device?view=iotedge-2018-06)

## Configure the device

To connect the device to the Azure IoT Hub, one needs to set the URL/Hostname of the IoT Hub and the root certificate of the IoT Hub as below.

Set the URL/Hostname of your Azure IoT Hub.   

```
sudo tedge config set azure.url your-iot-hub-name.azure-devices.net
```

The URL/Hostname can be found in the Azure web portal, clicking on the overview section of your IoT Hub.

Set the path to the root certificate if necessary. The default is `/etc/ssl/certs`.

```
sudo tedge config set azure.root.cert.path /etc/ssl/certs/Baltimore_CyberTrust_Root.pem
```

This will set the root certificate path of the Azure IoT Hub.
In most of the Linux flavors, the certificate will be present in /etc/ssl/certs. If not found download it from [here](https://www.digicert.com/kb/digicert-root-certificates.htm).

## Connect the device

Now, you are ready to get your device connected to Azure IoT Hub with `tedge connect az`.
This command configures the MQTT broker:
* to establish a permanent and secure connection to the Azure cloud,
* to forward local messages to the cloud and vice versa.

```
$ sudo tedge connect az

Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Saving configuration for requested bridge.

Restarting mosquitto service.

Awaiting mosquitto to start. This may take up to 5 seconds.

Persisting mosquitto on reboot.

Successfully created bridge connection!

Sending packets to check connection. This may take up to 10 seconds.

Received expected response message, connection check is successful.
```

## Sending your first telemetry data

Sending data to Azure is done using MQTT over topics prefixed with `az`.
Any messages sent on the topic will be forwarded to Azure.
Here, we use `tedge mqtt pub az/messages/events/` a message to be understood as a temperature of 20 Degree.

```
$ tedge mqtt pub az/messages/events/ '{"temperature": 20}'
```
To view the messages that were sent from the device to the cloud, follow this [document](https://docs.microsoft.com/en-us/azure/iot-hub/quickstart-send-telemetry-cli#create-and-monitor-a-device).

More info about sending telemetry to Azure can be found [here](https://docs.microsoft.com/en-us/azure/iot-hub/quickstart-send-telemetry-dotnet)

## Next Steps

You can now:
* learn how to [send various kind of telemetry data](send-thin-edge-data.md)
  using the cloud-agnostic [Thin-Edge-Json data format](../architecture/thin-edge-json.md),
* or have a detailed view of the [topics mapped to and from Azure](../references/bridged-topics.md#azure-mqtt-topics)
  if you prefer to use directly Azure specific formats and protocols.
