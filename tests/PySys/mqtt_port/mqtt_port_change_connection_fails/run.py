import sys
import time

from pysys.basetest import BaseTest


"""
Validate changing the mqtt port using the tedge command that fails without restarting the mqtt server

Given a configured system, that is configured with certificate created and registered in a cloud
When `tedge mqtt.port set` with `sudo`
When the `sudo tedge mqtt sub` tries to subscribe for a topic and fails to connect to mqtt server
When the `sudo tedge mqtt pub` tries to publish a message and fails to connect to mqtt server

"""


class MqttPortChangeConnectionFails(BaseTest):
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

        # publish a message
        mqtt_pub = self.startProcess(
            command=self.sudo,
            arguments=[
                self.tedge,
                "mqtt",
                "pub",
                "tedge/measurements",
                '{ "temperature": 25 }',
            ],
            stdouterr="mqtt_pub",
            # dont exit test if status is 1, as the error messages are needed for validation
            expectedExitStatus="==1",
        )

    def validate(self):
        self.assertGrep(
            "mqtt_pub.err", "ERROR: the message has not been published", contains=True
        )
        self.assertGrep(
            "mqtt_pub.err", "Error: failed to publish the message", contains=True
        )

    def mqtt_cleanup(self):
        # unset a new mqtt port, falls back to default port (1883)
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "mqtt.port"],
            stdouterr="mqtt_port_unset",
        )
