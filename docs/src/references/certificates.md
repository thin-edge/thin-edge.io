---
title: Certificate Management
tags: [Reference, Certificate, Security]
sidebar_position: 1
description: Certificate Management reference guide
---

## Integration with Cumulocity CA

Firstly, the feature needs to be enabled on the management tenant

```
c8y features enable --key certificate-authority
```

Then still on the management tenant, you can enable this for individual tenants using:

```
c8y features tenants enable --key certificate-authority --tenant txxxx
```

Afterwards, in your tenant you can then create the CA certificate using go-c8y-cli >= 2.50.0
(this command is idempotent, so if the ca cert already exists, then it will just return info about it)

```
c8y devicemanagement certificate-authority create
```

## Certificate request

Requesting a certificate from Cumulocity for a device is a two steps operation:

- The device has to be registered on the tenant, associating a one-time password to the device id.
- The device has to request its certificate from the tenant, using the registered device id and one-time password.

Device registration is done either directly on the tenant or using `go-c8y-cli` extension for thin-edge:
[`c8y-tedge`](https://github.com/thin-edge/c8y-tedge).

```sh title="on the operator laptop"
DEVICE_ID=device-007
DEVICE_ONE_TIME_PASSWORD=30271-2685-25572
c8y tedge cms register "$DEVICE_ID" "$DEVICE_ONE_TIME_PASSWORD"
```

The device can now request its certificate using the same device id and one-time password:

```sh title="on the device"
DEVICE_ID=device-007
DEVICE_ONE_TIME_PASSWORD=30271-2685-25572
sudo tedge cert download c8y --device-id "$DEVICE_ID" --one-time-password "$DEVICE_ONE_TIME_PASSWORD"
```

The operator has to give the *same* device id and one-time password *twice*, to the tenant and to the device.
This is the proof that the device can be trusted by the tenant and the tenant by the device,
*provided* the operator is trusted by the tenant and the device (i.e. has been granted the appropriate access privileges).

On success, a fresh new certificate is downloaded and ready to be used to connect Cumulocity.

```sh title="checking the certificate is signed by Cumulocity"
$ tedge cert show
Certificate:   /etc/tedge/device-certs/device-cert.pem
Subject:       CN=device-007, O=Thin Edge, OU=Device
Issuer:        C=United States, O=Cumulocity, CN=t9700           # <= signed by the tenant t9700
Status:        VALID (expires in: 11months 15days 47m 3s)
...
```

Note that the two steps, device registration on the tenant and certificate download request, can be done in a different order,
the device generating a one-time password that is communicated by the operator to the tenant
while the device keep trying to get its certificate.

## Certificate renewal

Once a %%te%% device is connected to Cumulocity, its certificate can be renewed from Cumulocity.

First be sure the device is actually connected
(because the proof that the device can be trusted is done using JWT tokens transmitted over MQTT using the current certificate):

```sh title="the device being connected to Cumulocity"
$ sudo tedge cert renew
Certificate renewed successfully
    For an un-interrupted service:
    => the device has to be reconnected to the cloud

Certificate:   /etc/tedge/device-certs/device-cert.pem.new           # <= The current cert has not been erased
Subject:       CN=device-007, O=Thin Edge, OU=Device
Issuer:        C=United States, O=Cumulocity, CN=t9700
Status:        VALID (expires in: 11months 30days 3h 50m 23s)        # <= The validity period has been extended
...
```

On success, a *new* certificate is ready to be used but the current certificate is kept active.
In order to make the new certificate active, the device has to be reconnected.

```sh title="Activate the renewed certificate"
$ sudo tedge reconnect c8y
...
Validating new certificate: /etc/tedge/device-certs/demo-device-888.pem.new... ✓
The new certificate is now the active certificate /etc/tedge/device-certs/demo-device-888.pem
...
```

Note that before making any certificate update, the `tedge connect` command checks the new certificate.
If for some reason the new certificate is rejected by Cumulocity, `tedge connect` proceeds with the former certificate.

## Automated certificate renewal

TODO: document https://github.com/thin-edge/thin-edge.io/pull/3523

The idea is to use `tedge cert needs-renewal` which return 0 only when the certificate must be renewed:

```sh
if tedge cert needs-renewal
then
  sudo tedge cert renew
  sudo tedge reconnect c8y 
fi
```

## Automated device on-boarding

TODO: explain how `c8y tedge cms register` and `tedge cert download c8y` can be combined
to build device images making the devices request their certificates on boot
with operator interactions reduced to security checks.