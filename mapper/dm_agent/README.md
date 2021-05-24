# Device Monitoring

Device monitoring is enabled on `thin-edge.io` using `collectd` and the `tedge-dm-agent` component.

# Prerequisites

Install collectd if not already installed: 

`sudo apt install collectd`

Configure collectd. Use the Thin Edge customized collectd configuration for basic monitoring: 

`sudo cp $TEDGE_REPO/configuration/contrib/collectd/collectd.conf /etc/collectd/collectd.conf`

Restart collectd after the config changes

`sudo systemctl restart collectd`

Validate if collectd is publishing stats to the MQTT broker

`mosquitto_sub -v -t collectd/#`

# Run Device Monitoring Agent

Run the dm-agent binary directly from `thin-edge.io` repository using cargo

`cargo run --bin tedge_dm_agent`

Validate if the collectd stats are being mapped to Thin Edge JSON and published back to the MQTT broker

`mosquitto_sub -v -t tedge/#`
