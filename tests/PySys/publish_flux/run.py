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

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_tedge"
        )

        if serv_mosq.exitStatus!=0:
            self.abort(FAILED)

    def execute(self):
        self.log.info("Execute")
        publisher = self.project.exampledir + "/sawtooth_publisher"
        cmd = os.path.expanduser(publisher)

        pub = self.startProcess(
            command = cmd,
            arguments=[ "100", "100", "5", "flux"],
            stdouterr="stdout"
        )

    def validate(self):
        self.log.info("Validate - Do it")


    def cleanup(self):
        self.log.info("Cleanup")
