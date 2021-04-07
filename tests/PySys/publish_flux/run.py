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

        publisher = self.project.exampledir + "/sawtooth_publisher"
        cmd = os.path.expanduser(publisher)

        pub = self.startProcess(
            command = cmd,
            arguments=[ "100", "100", "5", "flux"],
            stdouterr="stdout"
        )

    def validate(self):
        super().validate()
        self.log.info("Validate - Do it")


    def mycleanup(self):
        super().mycleanup()
        self.log.info("My Cleanup")
