import sys
import time

from environment_tedge import TedgeEnvironment

"""
Validate changing the mqtt port using the tedge command

Given a configured system
When `mqtt.port` is set using `tedge mqtt.port set` with `sudo`
When listed config using `tedge config list` the newly set port should be there.

"""


class MqttPortSet(TedgeEnvironment):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        self.addCleanupFunction(self.mqtt_cleanup)

    def execute(self):
        # set a new mqtt port for local communication
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "mqtt.port", "8880"],
            stdouterr="mqtt_port_set",
        )

    def validate(self):
        tedge_get = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "list"],
            stdouterr="tedge_get",
        )

        self.assertGrep("tedge_get.out", "mqtt.port=8880", contains=True)
        self.assertGrep("/etc/tedge/tedge.toml", "port = 8880", contains=True)

    def mqtt_cleanup(self):
        # unset a new mqtt port, falls back to default port (1883)
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "mqtt.port"],
            stdouterr="mqtt_port_unset",
        )

        # restart the tedge services
        self.tedge_connect_c8y()

        # disconnect the tedge services
        self.tedge_disconnect_c8y()
