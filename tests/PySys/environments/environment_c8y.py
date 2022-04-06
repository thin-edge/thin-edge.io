import base64
import json
import re
import requests

import psutil

from pysys.basetest import BaseTest
from pysys.constants import FAILED

from cumulocity import Cumulocity
from environment_tedge import TedgeEnvironment

"""
Environment to manage automated connects and disconnects to c8y
"""


class EnvironmentC8y(TedgeEnvironment):
    """
    Pysys Environment to manage automated connect and disconnect to c8y

    Tests that derive from class EnvironmentC8y use automated connect and
    disconnect to Cumulocity. Additional checks are made for the status of
    service mosquitto and service tedge-mapper.
    """

    cumulocity: Cumulocity

    def setup(self):
        self.log.debug("EnvironmentC8y Setup")
        super().setup()
        if self.project.c8yurl == "":
            self.abort(
                FAILED,
                "Cumulocity tenant URL is not set. Set with the env variable C8YURL",
            )
        if self.project.tenant == "":
            self.abort(
                FAILED,
                "Cumulocity tenant ID is not set. Set with the env variable C8YTENANT",
            )
        if self.project.c8yusername == "":
            self.abort(
                FAILED,
                "Cumulocity tenant username is not set. Set with the env variable C8YUSERNAME",
            )
        if self.project.c8ypass == "":
            self.abort(
                FAILED,
                "Cumulocity tenant password is not set. Set with the env variable C8YPASS",
            )
        if self.project.deviceid == "":
            self.abort(
                FAILED, "Device ID is not set. Set with the env variable C8YDEVICEID"
            )

        self.log.info("EnvironmentC8y Setup")
        self.addCleanupFunction(self.myenvcleanup)

        # Check if tedge-mapper is in disabled state
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper1",
            expectedExitStatus="==3",  # 3: disabled
        )

        # Connect the bridge
        self.tedge_connect_c8y()

        # Test the bridge connection
        self.tedge_connect_c8y_test()

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
            self.project.c8yurl,
            self.project.tenant,
            self.project.c8yusername,
            self.project.c8ypass,
            self.log,
        )

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

        # Disconnect Bridge
        self.tedge_disconnect_c8y()

        # Check if tedge-mapper is disabled
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper5",
            expectedExitStatus="==3",
        )
