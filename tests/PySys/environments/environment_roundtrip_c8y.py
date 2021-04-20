
import os
import pysys
from pysys.basetest import BaseTest

import sys

from environment_c8y import EnvironmentC8y

import time


class Environment_roundtrip_c8y(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)
        self.timeslot = 10

    def execute(self):
        super().execute()
        self.log.info("Execute")

        self.script = self.project.tebasedir + "ci/roundtrip_local_to_c8y.py"
        self.cmd = os.path.expanduser(self.script)

    def validate(self):
        super().validate()
        self.log.info("Validate")
        self.assertGrep("stdout.out", expr="Data verification PASSED", contains=True)
        self.assertGrep(
            "stdout.out", expr="Timestamp verification PASSED", contains=True
        )

    def mycleanup(self):
        super().mycleanup()
        self.log.info("MyCleanup")

