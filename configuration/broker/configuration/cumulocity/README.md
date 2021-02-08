# Connecting ThinEdge to Cumulocity

Here is a set of scripts to configure a MQTT channel between a device and Cumulocity.

If you have no certificate for your device, you can create one for testing purpose:

```
./create-self-signed-certificate.sh device-xyz xyz.crt xyz.key
```

Since a test certificate is self signed, you need to upload it on Cumulocity to be trusted:

```
./upload-certificate.sh device-xyz xyz.crt latest.stage.c8y.io t398942 alice
```

The certificate is then used to configure a secured bridge between the local MQTT broker and the Cumulocity MQTT endpoint.

```
./create-mosquitto-conf.sh latest.stage.c8y.io device-xyz xyz.crt xyz.key
```

You have then to run mosquitto with that configuration:

```
mosquitto -c mosquitto.conf 
```

The bridge can be tested with:
```
./test-bridge.sh
```

Once configured, the bridge:
* ensures that the cloud tenant is authenticated each time the bridge is open,
* uses the device certificate to authenticate the device,
* let any local client connects without any authentication,
* forwards the measurements, events, alarms and templates published on `c8y/#` topics to Cumulocity IoT.
* forwards the responses and operations received from Cumulocity to the corresponding `c8y/#` topics. 
* let the local clients use the non-Cumulocity topics as local communication channels.


See [Device integration using MQTT](https://cumulocity.com/guides/10.7.0-beta/device-sdk/mqtt/#device-certificates)

## Pre-requisite

A cumulocity tenant, user and password, plus an identifier for the device:

* C8Y: the c8y domain
* TENANT: the c8y tenant ID
* USER: the c8y user
* PASSWORD: ...
* DEVICE: an identifier for the device

Notes:

* Cumulocity version 10.7.0 onwards is required for certificate management.
* The scripts assume that mosquitto is installed on the device.
* The user and password are only used when a test certificate is generated,
  this certificate having to be uploaded on Cumulocity.

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

