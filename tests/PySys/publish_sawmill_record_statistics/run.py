import pysys
from pysys.constants import *
from pysys.basetest import BaseTest

# This test relies on a already working bridge

class PySysTest(BaseTest):
    def setup(self):
        self.log.info("Setup")
        self.addCleanupFunction(self.cleanup)

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["mosquitto", "status"],
            stdouterr="serv_mosq"
        )

        if serv_mosq.exitStatus!=0:
            self.abort(FAILED)

        # Check if tedge-mapper is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_tedge"
        )

        if serv_mosq.exitStatus!=0:
            self.abort(FAILED)

        proc_mos = self.startProcess(
            command="/usr/bin/pgrep",
            arguments=["-x", "mosquitto"],
            stdouterr="serv_mosq"
        )

    def execute(self):
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
        self.log.info("Validate - Do it")


    def cleanup(self):
        self.log.info("Cleanup")
