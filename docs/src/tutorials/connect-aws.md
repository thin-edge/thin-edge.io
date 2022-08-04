# Connect your device to AWS IoT

The very first step to enable **thin-edge.io** is to connect your device to the cloud.
* This is a 10 minutes operation to be done only once.
* It establishes a permanent connection from your device to the cloud end-point.
* This connection is secure (encrypted over TLS), and the two peers are identified by x509 certificates.
* Sending data to the cloud will then be as simple as sending data locally.

The focus is here on connecting the device to AWS IoT.
For connecting to other clouds:

- [Connecting to Cumulocity IoT](connect-c8y.md)
- [Connecting to Azure IoT](connect-azure.md)

Before you try to connect your device to AWS IoT, you need:

- Navigate to AWS IoT Core console on AWS.
- [Install `thin-edge.io` on your device](../howto-guides/002_installation.md).

## Create the certificate

The `tedge cert create` command creates a self-signed certificate which can be used for testing purpose.

A single argument is required: an identifier for the device.
This identifier will be used to uniquely identify your devices among others in your cloud tenant.
This identifier will be also used as the Common Name (CN) of the certificate.
Indeed, this certificate aims to authenticate that this device is the device with that identity.

```shell
$ sudo tedge cert create --device-id my-device
```

## Show certificate details

You can then check the content of that certificate.

```shell
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
To use a certificate signed by your Certificate Authority,
see the reference guide of [`tedge cert`](../references/tedge-cert.md).

## Register the device on AWS IoT

For a device to be trusted by AWS, one needs to add the self-signed certificate via the AWS IoT Console.

- In the AWS IoT service sidebar, navigate to **Manage -> All devices -> Things**.
- Click **Create things**
- **Create single thing**
- Enter the thing name which is the same as the `device-id` used in the step above
- Select **Unnamed shadow (classic)**. For now, the shadow is used to verify successful connection and registration of
  the device
- If using self-signed certificate generated with `tedge`, **Use my certificate -> CA is not registered with AWS IoT**,
  and upload the certificate in `/etc/tedge/device-certs`
- Create the policy that allows the client to connect, subscribe, and publish on any topics. For now we'll use the
  following policy (fill region and id fields of your tenant):

  ```json
  {
  "Version": "2012-10-17",
    "Statement": [
      {
        "Effect": "Allow",
        "Action": "*",
        "Resource": "arn:aws:iot:[region]:[id]:client/*"
      },
      {
        "Effect": "Allow",
        "Action": "*",
        "Resource": "arn:aws:iot:[region]:[id]:topic/*"
      },
      {
        "Effect": "Allow",
        "Action": "*",
        "Resource": "arn:aws:iot:[region]:[id]:topicfilter/*"
      }
    ]
  }
  ```

  **Be sure to correct the policy later for production environments.**

## Configure the device

To connect the device to the AWS IoT, one needs to set the URL/Hostname of the AWS IoT data-ats endpoint and the root
certificate as below.

Set the AWS IoT data-ats endpoint.

```shell
sudo tedge config set aws.url account-specific-prefix.iot.aws-region.amazonaws.com
```

The AWS IoT Data-ATS endpoint can be found [here](https://docs.aws.amazon.com/iot/latest/developerguide/iot-connect-devices.html#iot-connect-device-endpoints).

Set the path to the root certificate if necessary. The default is `/etc/ssl/certs`.

```shell
sudo tedge config set aws.root.cert.path /etc/ssl/certs/Baltimore_CyberTrust_Root.pem
```

This will set the root certificate path.
In most of the Linux flavors, the certificate will be present in /etc/ssl/certs.

## Connect the device

Now, you are ready to get your device connected to AWS IoT with `tedge connect aws`.
This command configures the MQTT broker:
* to establish a permanent and secure connection to the AWS cloud,
* to forward local messages to the cloud and vice versa.

Also, if you have installed `tedge_mapper`, this command starts and enables the tedge-mapper-aws systemd service.
At last, it sends packets to AWS IoT to check the connection.

```shell
$ sudo tedge connect aws
Checking if systemd is available.

Checking if configuration for requested bridge already exists.

Validating the bridge certificates.

Saving configuration for requested bridge.

Restarting mosquitto service.

Awaiting mosquitto to start. This may take up to 5 seconds.

Enabling mosquitto service on reboots.

Successfully created bridge connection!

Sending packets to check connection. This may take up to 2 seconds.

Received expected response message, connection check is successful.
Connection check is successful.
```

## Sending your first telemetry data

Here, we use `tedge mqtt pub tedge/measurements` a message to be understood as a temperature of 20 Degree.

```shell
$ tedge mqtt pub tedge/measurements '{"temperature": 20}'
```
