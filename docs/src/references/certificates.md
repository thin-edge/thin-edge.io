---
title: Certificate Management
tags: [Reference, Certificate, Security]
sidebar_position: 1
description: Certificate Management reference guide
---

import UserContext from '@site/src/components/UserContext';
import UserContextForm from '@site/src/components/UserContextForm';

:::tip
#### User Context {#user-context}

You can customize the documentation and commands shown on this page by providing
relevant settings which will be reflected in the instructions. It makes it even
easier to explore and use %%te%%.

<UserContextForm settings="C8Y_PROFILE_NAME,C8Y_PROFILE_URL,C8Y_URL,DEVICE_ID" />

The user context will be persisted in your web browser's local storage.
:::

## Integration with Cumulocity CA

Firstly, the Cumulocity Certificate Authority feature (called "certificate-authority") is currently in the Public Preview phase and hence needs to be enabled for a specific tenant before it can be used. The feature can be enabled from the tenant itself, or from the management tenant.

The following process can be used to enable the certificate-authority feature and to create the Certificate Authority Certificate which will be used to sign all of the device certificates either from the registration process or via the new certificate renewal API:

1. Enable the Cumulocity **certificate-authority** feature toggle

    Feature toggles can be controlled by using the [Cumulocity REST API](https://cumulocity.com/api/core/#tag/Feature-toggles-API), or by using [go-c8y-cli](https://goc8ycli.netlify.app/). For convenience, the go-c8y-cli commands are shown below:

    ```sh tab={"label":"From Current Tenant"}
    c8y features enable --key certificate-authority
    ```

    ```sh tab={"label":"From Management Tenant"}
    c8y features tenants enable --key certificate-authority --tenant txxxx
    ```

2. Create the Certificate Authority certificate

    The Certificate Authority certificate can be created using the UI, in the **Device Management** Application under

    *Management* &rarr; *Trusted Certificates* page, and then clicking the "Add CA Certificate" button (in the top right hand corner)


    Alternatively, you can use go-c8y-cli to also create the Certificate Authority certificate:

    ```sh
    c8y devicemanagement certificate-authority create
    ```

## Certificate request

Requesting a certificate from Cumulocity for a device is a two step operation:

- The device has to be registered on the tenant, associating a one-time password to the device id.
- The device has to request its certificate from the tenant, using the registered device id and one-time password.

### Using CLI Commands

1. Device registration is done either using the Cumulocity Device Management application or using [go-c8y-cli](https://goc8ycli.netlify.app/):

    <UserContext>

    ```sh title="on the operator laptop"
    c8y deviceregistration register-ca --id "$DEVICE_ID" --one-time-password "$DEVICE_ONE_TIME_PASSWORD"
    ```

    </UserContext>

    If you want to use an auto generated one-time password then you can use leave out the `--one-time-password` flag.

    <UserContext>

    ```sh title="on the operator laptop"
    c8y deviceregistration register-ca --id "$DEVICE_ID"
    ```

    </UserContext>

1. The device can now request its certificate using the same device id and one-time password:

    <UserContext>

    ```sh title="on the device"
    sudo tedge cert download c8y --device-id "$DEVICE_ID" --one-time-password "$DEVICE_ONE_TIME_PASSWORD"
    ```

    </UserContext>

    :::note
    The operator has to give the *same* device id and one-time password *twice*, to the tenant and to the device.
    This is the proof that the device can be trusted by the tenant and the tenant by the device,
    *provided* the operator is trusted by the tenant and the device (i.e. has been granted the appropriate access privileges).
    :::

    On success, a fresh new certificate is downloaded and ready to be used to connect Cumulocity.

1. Connect the devices using the newly downloaded certificate

    ```sh
    sudo tedge connect c8y
    ```

    You can check if the certificate is signed by the Cumulocity tenant, by using the following command and inspecting the **Issuer** field which should container the Cumulocity Tenant ID which was used to sign the device certificate.

    ```sh
    tedge cert show
    ```

    ```sh title="Output"
    Certificate:   /etc/tedge/device-certs/tedge-certificate.pem
    Subject:       CN=device-007, O=Thin Edge, OU=Device
    Issuer:        C=United States, O=Cumulocity, CN=t9700      # <= signed by the tenant t9700
    Status:        VALID (expires in: 11months 15days 47m 3s)
    ...
    ```

:::note
The two steps, device registration on the tenant and certificate download request, can be done in a different order,
the device generating a one-time password that is communicated by the operator to the tenant
while the device keep trying to download its certificate.
:::

## Certificate renewal

Once the %%te%% device is connected to Cumulocity, its certificate can be renewed by using the existing connection to Cumulocity. If the device is using SystemD as its init system, then the certificate certificate renewal process will be automatic, and it will be renewed before the certificate expires (the default is 30 days before expiration, but the value can be configured via the tedge configuration).

The Cumulocity certificate-authority feature can reissue a certificate provided the device can still communicate with the tenant it is requesting a certificate from. The certificate renewal is authenticated by way of a JWT which is requested over the MQTT connection using the current certificate. If for any reason the certificate expires, and the device can no longer communicate with Cumulocity, then the device will need to be registered again.

To ensure a smooth transition to the new certificate, the certificate renewal process is done in two distinct steps as described below:

1. Request a new certificate authenticating with the current certificate. The newly issued certificate is downloaded to the device, however it will not overwrite the current certificate. Instead the newly issued certificate will be saved next next to the current certificate and be given the `.new` filename suffix.
2. Verify the connection to Cumulocity using the new certificate candidate, and if the connection to Cumulocity is successful, then the new certificate candidate will overwrite the previous certificate. If the connection is not successful, then the existing certificate is not touched, and the certificate candidate will also remain untouched.

### Manually renewing the certificate

The certificate can be manually renewed by running the following commands, however the device MUST be connected to Cumulocity for the renewal to function.

```sh
sudo tedge cert renew
```

```text title="Output"
Certificate renewed successfully
    For an un-interrupted service:
    => the device has to be reconnected to the cloud

Certificate:   /etc/tedge/device-certs/tedge-certificate.pem.new     # <= The current cert has not been erased
Subject:       CN=device-007, O=Thin Edge, OU=Device
Issuer:        C=United States, O=Cumulocity, CN=t9700
Status:        VALID (expires in: 11months 30days 3h 50m 23s)  # <= The validity period has been extended
...
```

On success, a *new* certificate is ready to be used but the current certificate is kept active.
In order to make the new certificate active, the device has to be reconnected.

```sh
sudo tedge reconnect c8y
```

```text title="Output"
...
Validating new certificate: /etc/tedge/device-certs/tedge-certificate.pem.new... ✓
The new certificate is now the active certificate /etc/tedge/device-certs/tedge-certificate.pem
...
```

:::note
Before the replacing the current certificate, the `tedge connect` command checks the new certificate and verifies if it
be used to successfully connect to Cumulocity.
If for some reason the new certificate is rejected by Cumulocity, `tedge connect` proceeds with the former certificate.
:::

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