from pysys.basetest import BaseTest

import time
import os

"""
Validate local subscribing while no mosquitto is running

Given a configured system
When we stop mosquitto
When we subscribe to something
Then we expect an error code
Then we restart mosquitto
"""


class PySysTest(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"
        self.systemctl = "/usr/bin/systemctl"
        self.environ = {"HOME": os.environ.get("HOME")}

        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "mosquitto"],
            stdouterr="stop",
        )

        self.addCleanupFunction(self.mycleanup)

    def execute(self):

        pub = self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "sub", "atopic"],
            stdouterr="tedge_sub_fail",
            expectedExitStatus="==1",
            environs=self.environ,
        )

        # validate exit status with the expected status from calling startProcess
        self.assertThat("value" + pub.expectedExitStatus, value=pub.exitStatus)

    def mycleanup(self):
        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "mosquitto"],
            stdouterr="start",
        )
