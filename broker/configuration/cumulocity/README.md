# Connecting ThinEdge to Cumulocity

Here is a set of scripts to configure a MQTT channel between a device and Cumulocity.

The main script `connect-c8y.sh`:
* generates a PEM certificate for the device,
* ensures that this certificate is trusted by Cumulocity,
* configures the mosquitto MQTT broker to establish a secured bridge from the device to Cumulocity.

Once configured, the bridge:
* ensures that the cloud tenant is authenticated each time the bridge is open,
* uses the device certificate to authenticate the device
  (a password is required during the initialisation but is never used for MQTT),
* let any local client connects without any authentication,
* forwards the measurements, events, alarms and templates published on `c8y/#` topics to Cumulocity IoT.
* forwards the responses and operations received from Cumulocity to the corresponding `c8y/#` topics. 
* let the local clients use the non-Cumulocity topics as local communication channels.

The scripts might be used independently.
* `connect-c8y.sh`: main script
* `create-mosquitto-conf.s`: creates the mosquitto configuration (can also be used to configure a Docker image for mosquitto).
* `create-self-signed-certificate.sh`: creates a self-signed certificate.
* `enable-trusted-certificates.sh`: enables the 'Trusted Certificates' pannel in Cumulocity (TEMP: the panel is not enabled yet per default on the latest C8Y).
* `upload-certificate.sh`: uploads a certificate to be trusted by Cumulocity.
* `list-certificates.sh`: lists the certificates trusted ny Cumulocity.
* `test-bridge.sh`: is the bridge working properly?
* `get-credentials.sh`: kind of hack to pass the C8Y credentials from one script to another
  (WARNING: your credentials are cached in the file `.credentials` using http basic authentication).

## Pre-requisite

A cumulocity tenant, user and password, plus an identifier for the device:

* C8Y: the c8y endpoint
* TENANT: the c8y tenant ID
* USER: the c8y user
* PASSWORD: ...
* DEVICE: an identifier for the device

Notes:

* __Warning__: for certificate management, is required version 10.7.0 onwards of Cumulocity.
* On the beta release, the 'Trusted Certificates' pannel __must be enabled__ in Cumulocity using the script `enable-trusted-certificates.sh`.
* The scripts assume that mosquitto is installed on the devices.

## Connecting the device to Cumulocity

The script `connect-c8y.sh` requires the device identifier as a single argument.
It asks for the connection information unless previously cached in the `.credentials` file.

```
$ ./connect-c8y.sh my-edge-device
C8Y:latest.stage.c8y.io    
TENANT:t40270236
USER:didier
PASSWORD:
Creating the certificate
Creating mosquitto.conf
Uploading the certificate on Cumulocity
```

Should have been created:
* a device certificate, named after the device identifier (here `my-edge-device.crt`),
* a device private key (here `my-edge-device.key`),
* a trusted certificate on Cumulocity (using the device identifier),
* a `mosquitto.conf` configuration file using the freshly created certificate to authenticate the device.

## Running the bridge

The bridge is established by mosquitto:

```
$ mosquitto -c mosquitto.conf
1603903971: mosquitto version 1.6.9 starting
1603903971: Config loaded from mosquitto.conf.
1603903971: Opening ipv4 listen socket on port 1883.
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/s/ucr
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/s/ut/#
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/s/us
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/t/us
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/q/us
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/c/us
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/s/uc/#
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/t/uc/#
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/q/uc/#
1603903971: Bridge local.my-edge-device doing local SUBSCRIBE on topic c8y/c/uc/#
1603903971: Connecting bridge (step 1) edge_to_c8y (mqtt.latest.stage.c8y.io:8883)
1603903971: Connecting bridge (step 2) edge_to_c8y (mqtt.latest.stage.c8y.io:8883)
1603903971: Bridge my-edge-device sending CONNECT
...

```

Pitfalls
* Ensure no MQTT brocker is already listining on port 1883.
* This might require to stop the mosquitto daemon with `sudo service mosquitto stop`.

## Testing the bridge

The script `test-bridge.sh` check the bridge with a round trip between the device and the cloud:

```
$ ./test-bridge.sh 
[OK] sending and receiving data to and from c8y
[OK] the device certificate is a PEM file
[OK] the device certificate is trusted by c8y
```

The c8y topics are prefixed by `c8y/` and any messages publish to one of these sub-topics is forwarded unchanged to Cumulocity:

For instance, a temperature measurement can be published by the device using a local connection with no credentials:
```
mosquitto_pub -h 127.0.0.1  --topic c8y/s/us --message "211,21"
```

Here is the list of topics forwarded to Cumulocity:
* Subscription
  * `c8y/s/ucr`
* Templates
  * `c8y/s/ut/#`
* Static templates
  * `c8y/s/us`
  * `c8y/t/us`
  * `c8y/q/us`
  * `c8y/c/us`
* SmartRest 2.0
  * `c8y/s/uc/#`
  * `c8y/t/uc/#`
  * `c8y/q/uc/#`
  * `c8y/c/uc/#`

And the list of topics which can be locally subscribed to to receive responses and commands from Cumulocity:
* Subscription
  * `c8y/s/dcr`
* Templates
  * `c8y/s/dt`
* Static templates
  * `c8y/s/ds`
  * `c8y/s/os`
* Debug
  * `c8y/s/e`
* SmartRest 2.0
  * `c8y/s/dc/#`
  * `c8y/s/oc/#`

