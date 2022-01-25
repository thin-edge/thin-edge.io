

import psutil

from pysys.basetest import BaseTest
from pysys.constants import FAILED

from cumulocity import Cumulocity
from environment import TedgeEnvironment


class EnvironmentC8y(TedgeEnvironment):
    """
    Pysys Environment to manage automated connect and disconnect to c8y

    Tests that derive from class EnvironmentC8y use automated connect and
    disconnect to Cumulocity. Additional checks are made for the status of
    service mosquitto and service tedge-mapper.
    """

    def setup(self):
        self.log.debug("EnvironmentC8y Setup")

        if self.project.c8yurl == "":
            self.abort(
                FAILED, "Cumulocity tenant URL is not set. Set with the env variable C8YURL")
        if self.project.tenant == "":
            self.abort(
                FAILED, "Cumulocity tenant ID is not set. Set with the env variable C8YTENANT")
        if self.project.c8yusername == "":
            self.abort(
                FAILED, "Cumulocity tenant username is not set. Set with the env variable C8YUSERNAME")
        if self.project.c8ypass == "":
            self.abort(
                FAILED, "Cumulocity tenant password is not set. Set with the env variable C8YPASS")
        if self.project.deviceid == "":
            self.abort(
                FAILED, "Device ID is not set. Set with the env variable C8YDEVICEID")

        self.tedge = "/usr/bin/tedge"
        self.tedge_mapper_c8y = "tedge-mapper-c8y"
        self.sudo = "/usr/bin/sudo"
        self.systemctl = "/usr/bin/systemctl"
        self.log.info("EnvironmentC8y Setup")
        self.addCleanupFunction(self.myenvcleanup)

        # Check if tedge-mapper is in disabled state
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper1",
            expectedExitStatus="==3",  # 3: disabled
        )

        self.wait_if_restarting_mosquitto_too_fast()

        # Connect the bridge
        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="tedge_connect",
        )

        # Test the bridge connection
        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y", "--test"],
            stdouterr="tedge_connect_test",
        )

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", "mosquitto"],
            stdouterr="serv_mosq2",
        )

        # Check if tedge-mapper is active again
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper3",
        )

        self.cumulocity = Cumulocity(
            self.project.c8yurl, self.project.tenant, self.project.c8yusername, self.project.c8ypass, self.log)

    def execute(self):
        self.log.debug("EnvironmentC8y Execute")

    def validate(self):
        self.log.debug("EnvironmentC8y Validate")

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", "mosquitto"],
            stdouterr="serv_mosq",
        )

        # Check if tedge-mapper is active
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper4",
        )

    def myenvcleanup(self):
        self.log.debug("EnvironmentC8y Cleanup")

        self.wait_if_restarting_mosquitto_too_fast()
        # Disconnect Bridge
        disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="tedge_disconnect",
        )

        # Check if tedge-mapper is disabled
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper5",
            expectedExitStatus="==3",
        )

