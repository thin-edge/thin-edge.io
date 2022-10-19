# Getting started with thin-edge.io on a Raspberry Pi

After following this tutorial you will have an overview of the installation and configuration of thin-edge.io. As an example, a Raspberry Pi is used. This tutorial explains in small steps to reach the goal of sending data to an IoT cloud platform (in this case, Cumulocity IoT) and performing some additional device management tasks.


## Introduction

thin-edge.io is an open-source project to provide a cloud-agnostic edge framework. It is much more generic than the device management agent, so it can connect to multiple IoT cloud platforms, and it allows flexible logic executed on the device. It is optimized for a very small footprint and high performance.

The Raspberry PI is a relatively simple and cheap device but powerful. Therefore it is ideal for testing and try-outs and some production use cases.


##  Prerequisite

To follow this guide, you only need the following:
- A [Cumulocity](https://www.softwareag.cloud/site/product/cumulocity-iot.html) Trial tenant.

- A Raspberry Pi (3 or 4) with Raspian installed, for other boards and OS'es have a look [here](https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/supported-platforms.md)
- Updated device:
```
$ sudo apt-get update && sudo apt-get upgrade
```

## Steps

This tutorial is divided into small steps. The first three steps are needed to install and connect to an IoT cloud platform. The last three are optional but needed to get a good overview of the capabilities of thin-edge.io.

[Step 1 Install thin-edge.io](#Step-1-Install-thin-edge.io)

[Step 2 Configure and Connect to IoT Cloud](#Step-2-Configure-and-Connect-to-IoT-Cloud)

[Step 3 Sending Device Data](#Step-3-Sending-Device-Data)

[Step 4 Monitor your device](#Step-4-Monitor-your-device)

[Step 5 Add software management](#Step-5-Add-software-management)

[Step 6 Manage configuration files](#Step-5-Manage-configuration-files)

[Step 7 Manage Log-Files](#Step-6-Manage-Log-Files)




## Step 1 Install thin-edge.io

There are two ways to install thin-edge.io:
- Use a script
- Manually

The easiest way is to use the installation script with this command:
```
curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
```
This script will install the latest version of thin-edge.io with the following components:
- Mosquitto
- Command line Interface (CLI) tool
- Tedge mapper

If you want to manually install thin-edge.io or install another version, or upgrade the current version, please have a look [here](https://thin-edge.github.io/thin-edge.io/html/howto-guides/002_installation.html#thin-edgeio-manual-installation) for more information.

After a successful installation, you can now use thin-edge.io via the CLI and use the tedge commands.


### Tedge CLI

In the previous step, the CLI tool is installed, which is a very powerful

The usage is as follows:
```
tedge [OPTIONS] [SUBCOMMAND]
```
and ```-h``` can be used to see the help for the latest subcommand.

When running this command you should see something similar like the following:


```shell
$ sudo tedge -h

tedge             
tedge 0.7.7
tedge is the cli tool for thin-edge.io

USAGE:
    tedge [OPTIONS] [SUBCOMMAND]

OPTIONS:
        --config-dir <CONFIG_DIR>    [default: /etc/tedge]
    -h, --help                       Print help information
        --init                       Initialize the tedge
    -V, --version                    Print version information

SUBCOMMANDS:
    cert          Create and manage device certificate
    config        Configure Thin Edge
    connect       Connect to connector provider
    disconnect    Remove bridge connection for a provider
    help          Print this message or the help of the given subcommand(s)
    mqtt          Publish a message on a topic and subscribe a topic
```

Here you can find an [overview of the commands for the CLI tool](https://thin-edge.github.io/thin-edge.io/html/references/references.html).

The CLI will be used to configure the thin-edge.io installation on the device in the next steps.

## Step 2 Configure and Connect to IoT Cloud

To connect the device to the Cumulocity IoT Cloud it needs to be configured.
The following configuration parameters are mandatory

   ``` C8Y URL```

This URL is needed in order to allow the upload of the certificate to the specific tenant and the registration of the device. It can be configured via:
```
sudo tedge config set c8y.url {{YOUR_C8Y_URL}}
```
### Certificate

thin-edge.io connects via MQTT protocol using a X.509 certificate for authentication. To do so, a certificate must be trusted by Cumulocity. A certificate is trusted when it is added to the trusted certificates and is in an activated state.

First, we need to create the device certificate locally (If you already have a device certificate uploaded directly via the UI to Cumulocity you can skip that step).
```
sudo tedge cert create --device-id {{YOUR_UNIQUE_DEVICE_ID}}
```
- The device id is a unique identifier e.g. MAC address that identifies a physical device.

The certificate is uploaded to the Cumulocity Tenant via:
```
sudo tedge cert upload c8y --user {{YOUR_USERNAME}}
```
If the password prompt appears, enter your password.


### Connect

We now are ready to connect RevPi with Cumulocity. This can be achieved via:
```
sudo tedge connect c8y
```

When the connection is established, the device will be created on the platform side and can be found within the device list in the device management.


## Step 3 Sending Device Data

Once your device is configured and connected to an IoT cloud platform, you can start sending measurements, events or alarms. In the standard configuration, you can not connect externally to the mosquito broker and thus the messages have to be sent directly from the device itself. If you want to change that, you need to configure it according to [here](link to configure MQTT).

The tedge CLI allows you to send payloads via MQTT the following way:
```
tedge mqtt pub {{TOPIC}} {{PAYLOAD}}
```
thin-edge.io comes with a tedge-mapper daemon. This process collects the data from the ```tedge/#``` topics and translates them to the tedge payloads on the ```c8y/#``` topics which is mapped directly to Cumulocity. The mapper translates simple JSON to the desired target payload for Cumulocity.

### Sending measurements

Measurements within Cumulocity represent regularly acquired readings and statistics from sensors.

A simple single-valued measurement like a temperature measurement can be represented in Thin Edge JSON as follows:
```
{ "temperature": 25 }
```
With the key-value pair representing the measurement type and the numeric value of the measurement. The endpoint that is supervised by the tedge-mapper for measurements is:
```
tedge/measurements
```
The temperature measurement described above can be sent as follows:
```
tedge mqtt pub tedge/measurements '{ "temperature": 25 }'
```

### Sending events

Events are used to pass real-time information, which are not just plain sensor values, through Cumulocity IoT (or other IoT cloud platforms).

A simple event can be represented in Thin Edge JSON as follows:
```
{
    "text": "A door was closed",
    "time": "2022-06-10T05:30:45+00:00"
}
```
The endpoint that is supervised by the tedge-mapper for events is:
```
tedge/events/{{event-type}}
```
So the door open event described above can be sent as follows:
```
tedge mqtt pub tedge/events/door '{"text": "A door was closed","time": "2022-06-10T05:30:45+00:00"}'
```

## Step 4 Monitor your device

With thin-edge.io device monitoring, you can collect metrics from the device and forward these device metrics to Cumulocity IoT.

thin-edge.io uses the open source component ```collectd``` to collect the metrics from the device. thin-edge.io translates the ```collected``` metrics from their native format to the thin-edge.io JSON format and then into the cloud-vendor-specific format.

Enabling monitoring on your device is a 3-steps process:

- Install collectd
- Configure collectd
- Enable thin-edge.io monitoring

### Install collectd

Because thin-edge.io uses the MQTT plugin of collectd, installation of the Mosquitto client library (either libmosquitto1 or mosquitto-clients) is required.
```
sudo apt-get install libmosquitto1
```
To install collectd:
```
sudo apt-get install collectd-core
```
### Configure collectd

thin-edge.io provides a basic collectd configuration that can be used to collect CPU, memory and disk metrics.

Simply copy the file to the main collectd configuration file and restart the daemon.
```
sudo cp /etc/tedge/contrib/collectd/collectd.conf /etc/collectd/collectd.conf
sudo systemctl restart collectd
```
What you should see by now is that data arrives on the ```collectd/#``` topics. You can check that via:
```
tedge mqtt sub 'collectd/#'
```

### Enable Collectd

To enable monitoring on your device, you have to launch the ```tedge-mapper-collectd daemon``` process. This process collects the data from the ```collectd/#``` topics and translates them to the tedge payloads on the ```c8y/#``` topics.
```
sudo systemctl start tedge-mapper-collectd
sudo systemctl enable tedge-mapper-collectd
```
You can inspect the collected and translated metrics, by subscribing to these topics:
```
tedge mqtt sub 'c8y/#'
```
The monitoring data will appear in Cumulocity IoT on the device in the measurement section.

## Step 5 Add software management

Software management takes care of allowing installation and management of any type of software from the IoT cloud platform. Since the type is generic, any type of software can be managed. In thin-edge.io this can be extended with plugins. For every software type, a particular plugin is needed.

The following plugins do exist:

- Docker
- APT
- Docker-compose
- Snap

In order to use those plugins they need to be copied to:

```/etc/tedge/sm-plugins```

The APT plugin is installed automatically. You can find the other plugins in the repository. Make sure to disconnect/reconnect the device after adding plugins via:
```
sudo tedge disconnect c8y
sudo tedge connect c8y
```


How to [develop your own plugins](https://thin-edge.github.io/thin-edge.io/html/tutorials/write-my-software-management-plugin.html) is described here.

## Step 6 Manage configuration files

With thin-edge.io you can manage config files on a device by using the Cumulocity configuration management feature as a part of Device Management.

This functionality is directly installed with the initial script. However, you need to configure the ```/etc/tedge/c8y/c8y-configuration-plugin.toml``` and add the entries for the configuration files that you want to manage. Just copy the following content to that file:
```
files = [
    { path = '/etc/tedge/tedge.toml' },
    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },
    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },
    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' }
]
```
The daemon is started/enabled via:
```
sudo systemctl start c8y-configuration-plugin
sudo systemctl enable c8y-configuration-plugin
```

## Step 7 Manage Log-Files

With thin-edge.io you can request log files from a device by using the Cumulocity log request feature as a part of Device Management.

This functionality is also directly installed with the initial script. However, you need to configure the /etc/tedge/c8y/c8y-log-plugin.toml and add the entries for the log files that can be requested. Just copy the following content to that file:
```
files = [
  { type = "software-management", path = "/var/log/tedge/agent/software-*" },
  { type = "mosquitto", path = "/var/log/mosquitto/mosquitto.log" },
  { type = "daemon", path = "/var/log/daemon.log" },
  { type = "user", path = "/var/log/user.log" },
  { type = "apt-history", path = "/var/log/apt/history.log" },
  { type = "apt-term", path = "/var/log/apt/term.log" },
  { type = "auth", path = "/var/log/auth.log" },
  { type = "dpkg", path = "/var/log/dpkg.log" },
  { type = "kern", path = "/var/log/kern.log" }
]
```
The daemon is started/enabled via:
```
sudo systemctl start c8y-log-plugin
sudo systemctl enable c8y-log-plugin
```
If you add the ```c8y-log-plugin.toml``` into the ```c8y-configuration-plugin.toml``` you can to the administration from there.
However, keep in mind that the daemon has to be restarted every time the ```/etc/tedge/c8y/c8y-log-plugin.toml``` is touched via the command line.
