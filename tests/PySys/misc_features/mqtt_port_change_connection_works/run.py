import sys
import time
import subprocess
from pathlib import Path

from pysys.basetest import BaseTest

"""
Validate changing the mqtt port using the tedge command

Given a configured system, that is configured with certificate created and registered in a cloud
When the thin edge device is disconnected from cloud, `sudo tedge disconnect c8y`
When `tedge mqtt.port set` with sudo
When the thin edge device is connected to cloud, `sudo tedge connect c8y`
Now validate the services that use the mqtt port
   Validate tedge mqtt pub/sub
   Validate tedge connect c8y --test
   Validate tedge_mapper status
   Validate collectd_mapper status

"""


class MqttPortChangeConnectionWorks(TedgeEnvironment):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # disconnect from c8y cloud

        self.tedge_disconnect_c8y()
        # set a new mqtt port for local communication
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "mqtt.port", "8889"],
            stdouterr="mqtt_port",
        )
        self.addCleanupFunction(self.mqtt_cleanup)

    def execute(self):
        # connect to c8y cloud
        self.tedge_connect_c8y()

        # check connection
        self.tedge_connect_c8y_test()

        # subscribe for messages
        mqtt_sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "tedge/measurements"],
            stdouterr="mqtt_sub",
            background=True,
        )

    def validate(self):
        time.sleep(1)
        # validate tedge mqtt pub/sub
        self.validate_tedge_mqtt()
        # validate c8y connection
        self.assertGrep(
            "tedge_connect_c8y_test.out",
            "connection check is successful",
            contains=True,
        )
        # validate c8y mapper
        self.validate_tedge_mapper_c8y()

        # validate collectd mapper
        self.validate_collectd_mapper()

        # validate tedge agent
        self.validate_tedge_agent()

    def validate_tedge_mqtt(self):
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
        )

        # check if the file exists
        self.check_if_sub_logged()

        # Stop the subscriber
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

        self.assertGrep("mqtt_sub.out", '{ "temperature": 25 }', contains=True)

    def check_if_sub_logged(self):
        fout = Path(self.output + "/mqtt_sub.out")
        ferr = Path(self.output + "/mqtt_sub.err")
        n = 0
        while n < 10:
            if fout.is_file() or ferr.is_file():
                return
            else:
                time.sleep(1)
                n += 1
        self.assertFalse(True, abortOnError=True, assertMessage=None)

    def validate_tedge_mapper_c8y(self):
        # check the status of the c8y mapper
        c8y_mapper_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "status", "tedge-mapper-c8y.service"],
            stdouterr="c8y_mapper_status",
        )

        self.assertGrep(
            "c8y_mapper_status.out",
            " MQTT connection error: I/O: Connection refused (os error 111)",
            contains=False,
        )

    def validate_collectd_mapper(self):
        # restart the collectd mapper to use recently set port
        c8y_mapper_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "restart", "tedge-mapper-collectd.service"],
            stdouterr="collectd_mapper_restart",
        )

        # check the status of the collectd mapper
        c8y_mapper_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "status", "tedge-mapper-collectd.service"],
            stdouterr="collectd_mapper_status",
        )

        self.assertGrep(
            "collectd_mapper_status.out",
            " MQTT connection error: I/O: Connection refused (os error 111)",
            contains=False,
        )

    def validate_tedge_agent(self):
        # restart the tedge-agent to use recently set port
        tedge_agent_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "restart", "tedge-agent.service"],
            stdouterr="tedge_agent_restart",
        )

        # check the status of the tedge-agent
        tedge_agent_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "status", "tedge-agent.service"],
            stdouterr="tedge_agent_status",
        )

        self.assertGrep(
            "tedge_agent_status.out",
            " MQTT connection error: I/O: Connection refused (os error 111)",
            contains=False,
        )

    def mqtt_cleanup(self):

        # To leave the system in the previous working state
        # disconnect the device, unset the port, connect again and
        # disconnect again

        # disconnect Bridge
        self.tedge_disconnect_c8y()

        # unset a new mqtt port, falls back to default port (1883)
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "mqtt.port"],
            stdouterr="mqtt_port_unset",
        )

        # connect Bridge
        self.tedge_connect_c8y()

        # disconnect Bridge
        self.tedge_disconnect_c8y()
