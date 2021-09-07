import sys
import time
import subprocess

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


class MqttPortChangeConnectionWorks(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # disconnect from c8y cloud
        disconnect_c8y = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="disconnect_c8y",
        )
        self.addCleanupFunction(self.mqtt_cleanup)

    def execute(self):
        # set a new mqtt port for local communication
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "mqtt.port", "8889"],
            stdouterr="mqtt_port",
        )

        # wait for a while
        time.sleep(0.1)

        # connect to c8y cloud
        connect_c8y = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="connect_c8y",
        )

        time.sleep(0.1)
        # check connection
        connect_c8y = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y", "--test"],
            stdouterr="check_con_c8y",
        )

    def validate(self):
        time.sleep(1)
        # validate tedge mqtt pub/sub
        self.validate_tedge_mqtt()
        # validate c8y connection
        self.assertGrep("check_con_c8y.out",
                        "connection check is successful", contains=True)
        # validate c8y mapper
        self.validate_tedge_mapper_c8y()

        # validate collectd mapper
        self.validate_collectd_mapper()
        
        # validate tedge agent
        self.validate_tedge_agent()

    def validate_tedge_mqtt(self):
        self.start_subscriber()

        # publish a message
        mqtt_pub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub",
                       "tedge/measurements", "{ \"temperature\": 25 }"],
            stdouterr="mqtt_pub",
        )

        # wait for a while to write the log to filesystem
        time.sleep(6)
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

        self.assertGrep(
            "mqtt_sub.out", "{ \"temperature\": 25 }", contains=True)

    # Starting subscriber takes sometime, so instead of sleeping before moving
    # to next operation, its better to query active connections
    def start_subscriber(self):
        num_clients_b4 = subprocess.getoutput("mosquitto_sub -p 8889 -t '$SYS/broker/clients/active' -C 1")
        # subscribe for messages
        mqtt_sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "tedge/measurements"],
            stdouterr="mqtt_sub",
            background=True,
        )
        # mosquitto takes time to update the stats
        time.sleep(10)
        num_clients_after = subprocess.getoutput("mosquitto_sub -p 8889 -t '$SYS/broker/clients/active' -C 1")
        self.assertTrue(int(num_clients_after) > int(num_clients_b4), abortOnError=True, assertMessage=None)

    def validate_tedge_mapper_c8y(self):
        # check the status of the c8y mapper
        c8y_mapper_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "status", "tedge-mapper-c8y.service"],
            stdouterr="c8y_mapper_status",
        )

        self.assertGrep("c8y_mapper_status.out",
                        " MQTT connection error: I/O: Connection refused (os error 111)", contains=False)

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

        self.assertGrep("collectd_mapper_status.out",
                        " MQTT connection error: I/O: Connection refused (os error 111)", contains=False)

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

        self.assertGrep("tedge_agent_status.out",
                        " MQTT connection error: I/O: Connection refused (os error 111)", contains=False)

    def mqtt_cleanup(self):

        # To leave the system in the previous working state
        # disconnect the device, unset the port, connect again and
        # disconnect again

        # disconnect Bridge
        c8y_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="c8y_disconnect",
        )

        # unset a new mqtt port, falls back to default port (1883)
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "mqtt.port"],
            stdouterr="mqtt_port_unset",
        )

        # connect Bridge
        c8y_connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="c8y_connect",
        )

        # disconnect Bridge
        c8y_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="c8y_disconnect",
        )

