import sys
import time

from pysys.basetest import BaseTest


"""
Validate changing the mqtt port using the tedge command

Given a configured system
When mqtt.port is set using tedge mqtt.port set with sudo
When the tedge config is listed and searched for the port that has been set 

"""


class MqttPortUnSet(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # set a new mqtt port for local communication
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "mqtt.port", "8880"],
            stdouterr="mqtt_port_set",
        )

    def execute(self):
        # set a new mqtt port for local communication
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "mqtt.port"],
            stdouterr="mqtt_port_unset",
        )

    def validate(self):
        tedge_get = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "list"],
            stdouterr="tedge_get",
        )

        self.assertGrep("tedge_get.out", "mqtt.port=1883", contains=True)
