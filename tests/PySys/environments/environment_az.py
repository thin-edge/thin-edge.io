import pysys
from pysys.basetest import BaseTest

"""
Environment to manage automated connect and disconnect to az

Tests that derive from class EnvironmentAz use automated connect and
disconnect to Cumulocity. Additional checks are made for the status of
service mosquitto and service tedge-mapper.
"""


class EnvironmentAz(BaseTest):
    def setup(self):
        self.log.debug("EnvironmentAz Setup")

        self.tedge = "/usr/bin/tedge"
        self.tedge_mapper_az = "tedge-mapper-az"
        self.sudo = "/usr/bin/sudo"
        self.systemctl = "/usr/bin/systemctl"
        self.log.info("EnvironmentAz Setup")
        self.addCleanupFunction(self.myenvcleanup)

        # Check if tedge-mapper is in disabled state
        serv_mapper_az = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_az],
            stdouterr="serv_mapper1",
            expectedExitStatus="==3",  # 3: disabled
        )

        # Connect the bridge
        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "az"],
            stdouterr="tedge_connect",
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
            arguments=["status", self.tedge_mapper_az],
            stdouterr="serv_mapper3",
        )

    def execute(self):
        self.log.debug("EnvironmentAz Execute")

    def validate(self):
        self.log.debug("EnvironmentAz Validate")

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", "mosquitto"],
            stdouterr="serv_mosq",
        )

        # Check if tedge-mapper is active
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_az],
            stdouterr="serv_mapper4",
        )

    def myenvcleanup(self):
        self.log.debug("EnvironmentAz Cleanup")

        # Disconnect Bridge
        disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "az"],
            stdouterr="tedge_disconnect",
        )

        # Check if tedge-mapper is disabled
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_az],
            stdouterr="serv_mapper5",
            expectedExitStatus="==3",
        )
