import pysys
from pysys.basetest import BaseTest

"""
Environment to manage automated connect and disconnect to c8y

Tests that derive from class EnvironmentC8y use automated connect and
disconnect to Cumulocity. Additional checks are made for the status of
service mosquitto and service tedge-mapper.
"""


class EnvironmentC8y(BaseTest):
    def setup(self):

        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        self.log.info("EnvironmentC8y Setup")
        self.addCleanupFunction(self.mycleanup)

        # Check if tedge-mapper is in active
        serv_mapper = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_mapper1",
            expectedExitStatus="==3",
        )

        if serv_mapper.exitStatus != 3:
            self.log.error("The tedge-mapper service is running")
            self.abort(FAILED)

        # Connect the bridge
        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="tedge_connect",
            expectedExitStatus="==0",
        )

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["mosquitto", "status"],
            stdouterr="serv_mosq2",
            expectedExitStatus="==0",
        )

        if serv_mosq.exitStatus != 0:
            self.log.error("The Mosquitto service is not running")
            self.abort(FAILED)

        # Check if tedge-mapper is active again
        serv_mapper = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_mapper3",
        )

        if serv_mapper.exitStatus != 0:
            self.log.error("The tedge-mapper service is not running")
            self.abort(FAILED)


    def execute(self):
        self.log.info("EnvironmentC8y Execute")

    def validate(self):
        self.log.info("EnvironmentC8y Validate")

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["mosquitto", "status"],
            stdouterr="serv_mosq",
            expectedExitStatus="==0",
        )

        if serv_mosq.exitStatus != 0:
            self.log.error("The Mosquitto service is not running")
            self.abort(FAILED)

        # Check if tedge-mapper is active
        serv_mapper = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_mapper1",
            expectedExitStatus="==0",
        )

        if serv_mapper.exitStatus != 0:
            self.log.error("The tedge-mapper service is not running")
            self.abort(FAILED)

    def mycleanup(self):
        self.log.info("EnvironmentC8y Cleanup")

        # Disconnect Bridge
        disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="tedge_disconnect",
            expectedExitStatus="==0",
        )

        # Check if tedge-mapper is inactive
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_mapper2",
            expectedExitStatus="==3",
        )

        if serv_mosq.exitStatus != 3:
            self.log.error("The tedge-mapper service is running")
            self.abort(FAILED)
