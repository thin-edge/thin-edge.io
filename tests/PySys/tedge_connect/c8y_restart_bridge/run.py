import sys

sys.path.append("environments")
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


class RestartBridge(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        super().execute()
        self.log.info("Execute (empty in this case as we test the c8y enviroment)")

    def validate(self):
        super().validate()
        self.log.info("Validate")
        self.assertGrep(
            "tedge_connect_c8y.out", "Connection check is successful.", contains=True
        )
        fail = "Warning: Bridge has been configured, but Cumulocity connection check failed."
        self.assertGrep("tedge_connect_c8y.out", fail, contains=False)

    def mycleanup(self):
        self.log.info("MyCleanup")
