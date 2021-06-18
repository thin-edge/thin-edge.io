from pysys.basetest import BaseTest

import time

"""
Validate changing the mqtt port using the tedge command

Given a configured system, that is configured with certificate created and registered in a cloud
When the thin edge device is disconnected from cloud, sudo tedge disconnect c8y/az
When mqtt.port is set using tedge with sudo
When the thin edge device is connected to cloud, sudo tedge connect c8y/az
Now validate the services that use the mqtt port
   Validate tedge config set mqtt.port
   Validate tedge mqtt pub/sub
   Validate tedge connect c8y/az --test
   Validate tedge_mapper status

"""


class MonitoringWithSimulatedMessages(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # disconnect from c8y cloud
        collectd = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="disconnect_c8y",
        )

        # disconnect from az cloud
        collectd = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "az"],
            stdouterr="disconnect_az",
        )

        self.addCleanupFunction(self.monitoring_cleanup)

    def execute(self):
        # set a new mqtt port for local communication
        mqtt_port = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "8889"],
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

        # connect to az cloud
        connect_az = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "az"],
            stdouterr="connect_az",
        )

    def validate(self):
        # validate the mqtt port set
        self.validate_mqtt_port_set()
        # validate tedge mqtt pub/sub
        self.validate_tedge_mqtt()
        # validate c8y connection
        self.assertGrep("connect_c8y.out",
                        "connection check is successful", contains=True)
        # validate az connection
        self.assertGrep("connect_az.out",
                        "connection check is successful", contains=True)
        # validate c8y mapper
        self.validate_tedge_mapper_c8y()
        # validate az mapper
        self.validate_tedge_mapper_az()

    def validate_mqtt_port_set(self):
        # subscribe for messages
        tedge_get = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "list"],
            stdouterr="tedge_get",
        )

        self.assertGrep(
            "tedge_get.out", "8889", contains=True)

    def validate_tedge_mqtt(self):
        # subscribe for messages
        mqtt_sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "tedge/measurements"],
            stdouterr="mqtt_sub",
        )

        # publish a message
        mqtt_pub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub",
                       "tedge/measurements", "{ \"temperature\": 25 }"],
            stdouterr="mqtt_pub",
        )

        # wait for a while
        time.sleep(0.1)
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

        self.assertGrep(
            "mqtt_sub.out", "{ \"temperature\": 25 }", contains=True)

    def validate_tedge_mapper_c8y(self):
        # check the status of the c8y mapper
        c8y_mapper_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "status", "tedge-mapper-c8y.service"],
            stdouterr="c8y_mapper_status",
        )

        self.assertGrep("c8y_mapper_status.out",
                        " MQTT connection error: I/O: Connection refused (os error 111)", contains=False)

    def validate_tedge_mapper_az(self):
        # check the status of the az mapper
        az_mapper_status = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "status", "tedge-mapper-az.service"],
            stdouterr="az_mapper_status",
        )

        self.assertGrep("az_mapper_status.out",
                        " MQTT connection error: I/O: Connection refused (os error 111)", contains=False)
