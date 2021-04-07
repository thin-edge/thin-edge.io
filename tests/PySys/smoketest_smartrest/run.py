import pysys
from pysys.constants import *
from pysys.basetest import BaseTest

import sys
sys.path.append('environments')
from environment_c8y import EnvironmentC8y

import time

# This test relies on a already working bridge

class PySysTest(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

        # bad hack
        time.sleep(20)

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["mosquitto", "status"],
            stdouterr="serv_mosq"
        )

        if serv_mosq.exitStatus!=0:
            self.abort(FAILED)

    def execute(self):
        super().execute()
        self.log.info("Execute")

        script = self.project.tebasedir + "ci/roundtrip_local_to_c8y.py"
        cmd = os.path.expanduser(script)

        sub = self.startPython(
            arguments=[ cmd,
                "-m", "JSON",
                "-pub", self.project.exampledir,
                "-u", self.project.username,
                "-t", self.project.tennant,
                "-pass", self.project.c8ypass,
                "-id", self.project.deviceid],
            stdouterr="stdout",
        )

    def validate(self):
        super().validate()
        self.log.info("Validate")
        self.assertGrep('stdout.out', expr='Data verification PASSED', contains=True)
        self.assertGrep('stdout.out', expr='Timestamp verification PASSED', contains=True)

    def mycleanup(self):
        super().mycleanup()
        self.log.info("MyCleanup")
