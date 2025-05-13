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

## Overview

%%te%% supports full lifecycle management of device certificates, from device registration to certificate automatic renewal.

To interact with the Certificate Authority (CA), %%te%% uses the [EST protocol](https://en.wikipedia.org/wiki/Enrollment_over_Secure_Transport)
which rules certificate signing requests and responses in a standard manner.
However, for these requests to be accepted as legitimate,
the devices must be first registered through a channel which is specific to the Certificate Authority.
And, as of now, this specific is only implemented for Cumulocity.

## Integration with Cumulocity CA {#cumulocity-certificate-authority}

Firstly, the [Cumulocity Certificate Authority](https://cumulocity.com/docs/device-certificate-authentication/certificate-authority/) feature is currently in the Public Preview phase and hence needs to be enabled for a specific tenant before it can be used. The feature can be enabled from the tenant itself, or from the management tenant.

### Enable Cumulocity Certificate Authority {#enable-cumulocity-certificate-authority}

The following process can be used to enable the certificate-authority feature and to create the Certificate Authority Certificate which will be used to sign all of the device certificates either from the registration process or via the new certificate renewal API:

1. Enable the Cumulocity **certificate-authority** feature toggle

    Feature toggles can be controlled by using the [Cumulocity REST API](https://cumulocity.com/api/core/#tag/Feature-toggles-API), or by using [go-c8y-cli](https://goc8ycli.netlify.app/). For convenience, the go-c8y-cli commands are shown below:

    ```sh tab={"label":"From Current Tenant"}
    c8y features enable --key certificate-authority
    ```

    ```sh tab={"label":"From Management Tenant"}
    c8y features tenants enable --key certificate-authority --tenant txxxx
    ```

    :::note
    If you get a Permission Denied (403) error, then you need to add the "Tenant Manager" role to your user.
    :::

2. Create the Certificate Authority certificate

    The Certificate Authority certificate can be created using the UI, in the **Device Management** Application under

    *Management* &rarr; *Trusted Certificates* page, and then clicking the "Add CA Certificate" button (in the top right hand corner)


    Alternatively, you can use go-c8y-cli to also create the Certificate Authority certificate:

    ```sh
    c8y devicemanagement certificate-authority create
    ```

## Device Registration {#device-registration}

The device registration process provides an easy way for a device to get its initial device (x509) certificate which is used to establish a connection to Cumulocity.

The device registration is a two step operation:

- The device has to be registered on the tenant, associating a one-time password to the device id.
- The device has to request its certificate from the tenant, using the registered device id and one-time password.

:::tip
The two steps, device registration on the tenant and certificate download request, can be done in a different order,
the device generating a one-time password that is communicated by the operator to the tenant
whilst the device keeps trying to download its certificate.
:::

For a guide on how to register the device, please checkout the [Connecting to Cumulocity](../../operate/c8y/connect) guide.

### Using Device Management UI

For instructions on how to register a device using the Cumulocity Device Management UI, please see the [Connecting to Cumulocity](../../operate/c8y/connect) guide.

### Using CLI Commands

Below describes the steps to register a new device using a combination of [go-c8y-cli](https://goc8ycli.netlify.app/) commands and %%te%%.

:::note
go-c8y-cli simply uses the Cumulocity REST API to perform the actions, this means that you can also do the same steps by using the API directly.
:::

1. Register the device using [go-c8y-cli](https://goc8ycli.netlify.app/)

    <UserContext>

    ```sh title="on the operator laptop"
    c8y deviceregistration register-ca --id "$DEVICE_ID"
    ```

    </UserContext>

    If you want don't want to use an auto generated one-time password then you can provide your own value via the `--one-time-password` flag.

    <UserContext>

    ```sh title="on the operator laptop"
    c8y deviceregistration register-ca \
        --id "$DEVICE_ID" \
        --one-time-password "$DEVICE_ONE_TIME_PASSWORD"
    ```

    </UserContext>

1. On the device, run the following commands to set both the Cumulocity tenant's URL and to download the device's  certificate

    <UserContext>

    ```sh title="on the device"
    sudo tedge config set c8y.url "$C8Y_URL"
    sudo tedge cert download c8y --device-id "$DEVICE_ID" --one-time-password "$DEVICE_ONE_TIME_PASSWORD"
    ```

    </UserContext>

    :::note
    The operator has to give the *same* device id and one-time password *twice*, to the tenant and to the device.
    This is the proof that the device can be trusted by the tenant and the tenant by the device,
    *provided* the operator is trusted by the tenant and the device (i.e. has been granted the appropriate access privileges).
    :::

1. Connect %%te%% to Cumulocity

    ```sh
    sudo tedge connect c8y
    ```

    You can check if the certificate is signed by the Cumulocity tenant, by using the following command and inspecting the **Issuer** field which should container the Cumulocity Tenant ID which was used to sign the device certificate.

    ```sh
    tedge cert show
    ```

    <UserContext language="text" title="Output">

    ```sh
    Certificate:   /etc/tedge/device-certs/tedge-certificate.pem
    Subject:       CN=$DEVICE_ID, O=Thin Edge, OU=Device
    Issuer:        C=United States, O=Cumulocity, CN=t9700      # <= signed by the tenant t9700
    Status:        VALID (expires in: 11months 15days 47m 3s)
    ...
    ```
    </UserContext>

## Configuration {#configuration}

The following shows the %%te%% configuration options that can be used to control the registration and renewal process.

```sh
tedge config list certificate --doc
```

```text title="Output"
certificate.validity.requested_duration  Requested validity duration for a new certificate.
                                         Note: The CA might return certificates valid for period shorter than requested
                                         Example: 365d
  certificate.validity.minimum_duration  Minimum validity duration below which a new certificate should be requested.
                                         Note: This is an advisory setting and the renewal has to be scheduled
                                         Example: 30d
               certificate.organization  Organization name used for certificate signing requests.
                                         Example: ACME
          certificate.organization_unit  Organization unit used for certificate signing requests.
                                         Example: IoT
```

### certificate.validity.minimum_duration

The minimum duration setting controls when the exit-code of the `tedge cert needs-renewal` command allowing users to programmatically check if the certificate is "about to expire" and hence can trigger request a new certificate.

:::caution
If it is normal for devices to be disconnected from the cloud for extended periods of time (e.g. > 1 week), then it is strongly recommend that the minimum duration (`certificate.validity.minimum_duration`) be at least 3 times the maximum offline period to ensure that the device has enough time to renew its certificate whilst the device is able to communicate with Cumulocity. Otherwise, the device is at risk of failing to renew its certificate before it expires which would require the device to go through the [registration process](#device-registration) again.
:::

## Certificate renewal

Once the %%te%% device is connected to Cumulocity, its certificate can be renewed by using the existing connection to Cumulocity. If the device is using SystemD as its init system, then the certificate certificate renewal process will be automatic, and it will be renewed before the certificate expires (the default is 30 days before expiration, but the value can be configured via the tedge configuration).

The Cumulocity certificate-authority feature provides a new API endpoint for devices to request a new certificate provided the device can still communicate with the tenant it is requesting a certificate from. The certificate renewal is authenticated by way of a JWT which is requested over the MQTT connection using the current certificate. If for any reason the certificate expires, and the device can no longer communicate with Cumulocity, then the device will need to be registered again.

To ensure a smooth transition to the new certificate, the certificate renewal process is done in two distinct steps as described below:

1. Request a new certificate authenticating with the current certificate. The newly issued certificate is downloaded to the device, however it will not overwrite the current certificate. Instead the newly issued certificate will be saved next to the current certificate and be given the `.new` filename suffix.
2. Verify the connection to Cumulocity using the new certificate candidate, and if the connection to Cumulocity is successful, then the new certificate candidate will overwrite the previous certificate. If the connection is not successful, then the existing certificate is not touched, and the certificate candidate will also remain untouched.

For additional options on how the certificate registration and renewal can be configured (e.g. how many days before the certificate expires should the certificate be renewed), check out the [Configuration](#configuration) section.

### Automated certificate renewal (systemd)

If you're using a device with SystemD, connected to Cumulocity, then the certificate renewal service will be installed, and it will take care of renewing the device certificate automatically.

The automated certificate renewal is implemented using SystemD and the components and their role are listed below:

|SystemD Unit|Description|
|------------|-----------|
|tedge-cert-renewer@c8y.timer|Timer which runs periodically to trigger the `tedge-cert-renewer@c8y` service. By default it runs every hour on the hour (with built-in jitter).|
|tedge-cert-renewer@c8y.service|Service which performs the renewal if the certificate is about to expire as defined in the %%te%% configuration|

**tedge-cert-renewer@c8y.service**

The certificate renewal service is implemented using standard SystemD functionality which makes the process transparent (observable from systemd commands like systemctl and journalctl) and also customizable (using `systemctl edit tedge-cert-renewer@c8y.service`).

Below shows the different elements to the renewal service and the function of each.

|Step|SystemD Option|Purpose|
|----|-------|:-------|
|1. Pre condition|ExecCondition|Check if the certificate needs renewal (based on the expiration date)|
|2. Renew|ExecStartPre|Request a new certificate (reusing the existing cloud certificate and connection)|
|3. Verify|ExecStart|Reconnect using the new certificate to check if it can be used to connect to the cloud|
|4. Cleanup|ExecStopPost|Remove failed certificates if they did not pass the verification step|

:::note
Please consult the [SystemD manual](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html) for a detailed description of each of the SystemD options and how they interact with each other.
:::

#### Check Renewal Status

You can check the certificate renewal timer to check that it is active and when the next trigger is scheduled to occur.

```sh
sudo systemctl status tedge-cert-renewer@c8y.timer
```

<UserContext language="text" title="Output">

```
● tedge-cert-renewer@c8y.timer - Timer for thin-edge.io certificate renewal of c8y
     Loaded: loaded (/lib/systemd/system/tedge-cert-renewer@.timer; disabled; preset: enabled)
     Active: active (waiting) since Mon 2025-05-05 22:20:15 BST; 5 days ago
    Trigger: Sun 2025-05-11 09:00:18 BST; 30min left
   Triggers: ● tedge-cert-renewer@c8y.service
       Docs: https://thin-edge.io

May 05 22:20:15 $DEVICE_ID systemd[1]: Started tedge-cert-renewer@c8y.timer - Timer for thin-edge.io certificate renewal of c8y.
```

</UserContext>

The status of the certificate renewal service can also be inspected using the following command:

```sh
sudo systemctl status tedge-cert-renewer@c8y.service
```

<UserContext language="text" title="Output">

```
○ tedge-cert-renewer@c8y.service - thin-edge.io certificate renewer for c8y
     Loaded: loaded (/lib/systemd/system/tedge-cert-renewer@.service; disabled; preset: enabled)
     Active: inactive (dead) (Result: exec-condition) since Sun 2025-05-11 08:00:14 BST; 30min ago
TriggeredBy: ● tedge-cert-renewer@c8y.timer
  Condition: start condition failed at Sun 2025-05-11 08:00:13 BST; 30min ago
    Process: 77871 ExecCondition=/usr/bin/tedge cert needs-renewal c8y (code=exited, status=1/FAILURE)
    Process: 77877 ExecStopPost=sh -c rm -f "$(tedge config get c8y.device.cert_path).new" (code=exited, status=0/SUCCESS)
        CPU: 15ms

May 11 08:00:14 $DEVICE_ID systemd[1]: Starting tedge-cert-renewer@c8y.service - thin-edge.io certificate renewer for c8y...
May 11 08:00:14 $DEVICE_ID tedge[77871]: Status:        VALID (expires in: 11months 28days 12h 8m 15s)
May 11 08:00:14 $DEVICE_ID systemd[1]: tedge-cert-renewer@c8y.service: Skipped due to 'exec-condition'.
May 11 08:00:14 $DEVICE_ID systemd[1]: Condition check resulted in tedge-cert-renewer@c8y.service - thin-edge.io certificate renewer for c8y being skipped.
```

</UserContext>

The example above shows that the current device's certificate is still valid for the next 11 months, so there is no need to renew the certificate.


### Manually renewing the certificate

The certificate can be manually renewed by running the following commands, however the device MUST be connected to Cumulocity for the renewal to function.

```sh
sudo tedge cert renew
```

<UserContext language="text" title="Output">

```
Certificate renewed successfully
    For an un-interrupted service:
    => the device has to be reconnected to the cloud

Certificate:   /etc/tedge/device-certs/tedge-certificate.pem.new     # <= The current cert has not been erased
Subject:       CN=$DEVICE_ID, O=Thin Edge, OU=Device
Issuer:        C=United States, O=Cumulocity, CN=t9700
Status:        VALID (expires in: 11months 30days 3h 50m 23s)  # <= The validity period has been extended
...
```

</UserContext>

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

### Creating your own renewal logic

If you need to implement your own certificate renewal logic, or just need to trigger the renew from another init. system or as a cron job, then you can still re-use a lot of the tedge commands.

The following example shows the general structure of the code needed to renew  the certificate using the Cumulocity certificate authority. The script snippet can be called from an non-SystemD init system (e.g. SysVinit, OpenRC etc.), or from a cron job:

```sh
if tedge cert needs-renewal
then
    sudo tedge cert renew
    sudo tedge reconnect c8y
fi
```

:::note
The `tedge cert needs-renewal` command will return a zero exit code when the certificate needs to be renewed.
:::

Alternatively, if the certificate renewal process involves communicating with an external service, then you can execute the required external API call rather than using the `tedge cert renew` command. Below shows an example of such a script which calls an external microservice which is hosted within Cumulocity.

```sh
if tedge cert needs-renewal
then
    echo "Certificate is about to expire, trying to renew it"

    echo "Create a Certificate Signing Request file"
    tedge cert create-csr --output-path /tmp/device.csr

    # Call some external service (in this case it is calling a service hosted in Cumulocity)
    # The new certificate is written to file with the .new extension
    echo "Calling an external service to renew the certificate"
    NEW_CERTIFICATE="$(tedge config get device.cert_path).new" 
    sudo tedge http api post '/c8y/service/my_custom_microservice/renew' --file "/tmp/device.csr" | sudo tee "$NEW_CERTIFICATE"

    # Reconnect and verify the new certificate. tedge will only replace the existing certificate if the .new cert
    # can be used to establish a connection successfully
    sudo tedge reconnect c8y

    echo "Cleanup the new certificate (if it exists)"
    sudo rm -f "$NEW_CERTIFICATE"
fi
```
