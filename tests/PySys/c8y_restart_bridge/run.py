import pysys
from pysys.constants import *
from pysys.basetest import BaseTest

import sys
sys.path.append('environments')
from environment_c8y import EnvironmentC8y

"""
Bridge restart

Given a configured system with configured certificate
When we setup EnvironmentC8y
When we validate EnvironmentC8y
When we cleanup EnvironmentC8y
Then then the test has passed

So far just use the EnvironmentC8y nothing else:
see ../environment/environment_c8y
"""

class PySysTest(EnvironmentC8y):

    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        super().execute()
        self.log.info("Execute")

    def validate(self):
        super().validate()
        self.log.info("Validate")
        self.addOutcome(PASSED)

    def mycleanup(self):
        super().mycleanup()
        self.log.info("MyCleanup")
