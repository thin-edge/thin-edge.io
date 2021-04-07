import pysys
from pysys.constants import *
from pysys.basetest import BaseTest

import sys
sys.path.append('environments')
from environment_c8y import EnvironmentC8y

# This test relies on a already working bridge

class PySysTest(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        super().execute()
        self.log.info("Execute")

        sub = self.startProcess(
            command = "/usr/bin/mosquitto_sub",
            arguments=["-v", "-h", "localhost", "-t", "$SYS/#"],
            stdouterr="mosquitto_sub_stdout",
            background=True,
        )

        stats_mosquitto = self.startProcess(
            command = "/bin/sh",
            arguments = ["-c", "while true; do cat /proc/$(pgrep -x mosquitto)/status; sleep 1; done"],
            stdouterr="stats_mosquitto_stdout",
            background=True,
        )

        stats_mapper = self.startProcess(
            command = "/bin/sh",
            arguments = ["-c", "while true; do cat /proc/$(pgrep -x tedge_mapper)/status; sleep 1; done"],
            stdouterr="stats_mapper_stdout",
            background=True,
        )

        publisher = self.project.exampledir + "/sawtooth_publisher"
        cmd = os.path.expanduser(publisher)

        pub = self.startProcess(
            command = cmd,
            # run for one minute
            arguments=[ "100", "100", "6", "sawmill"],
            stdouterr="stdout_sawmill"
        )

    def validate(self):
        super().validate()
        self.log.info("Validate - Do it")


    def mycleanup(self):
        super().mycleanup()
        self.log.info("My Cleanup")
