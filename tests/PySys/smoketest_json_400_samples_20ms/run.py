import os
import pysys
from pysys.basetest import BaseTest

import sys

sys.path.append("environments")
from environment_roundtrip_c8y import Environment_roundtrip_c8y

import time

"""
Roundtrip test C8y 400 samples 20ms delay

Given a configured system with configured certificate
When we derive from EnvironmentC8y
When we run the smoketest for JSON publishing with defaults a size of 400, 20ms delay
Then we validate the data from C8y
"""


class PySysTest(Environment_roundtrip_c8y):
    def setup(self):
        super().setup()

        # bad hack to wait until the receive window is empty again
        time.sleep(self.timeslot)

    def execute(self):
        super().execute()
        self.log.info("Execute")

        sub = self.startPython(
            arguments=[
                self.cmd,
                "-m",
                "JSON",
                "-pub",
                self.project.exampledir,
                "-u",
                self.project.username,
                "-t",
                self.project.tenant,
                "-pass",
                self.project.c8ypass,
                "-id",
                self.project.deviceid,
                "-o",
                "15",  # burst should take 8000ms
                "-d",
                "20",  # delay in ms
                "-s",
                "400",  # samples
            ],
            stdouterr="stdout",
        )

    def validate(self):
        super().validate()

    def mycleanup(self):
        super().mycleanup()

