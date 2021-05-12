import sys

sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Test `tedge connect c8y --test` (successful case):

Given a configured system with configured certificate
When we setup EnvironmentC8y
When we execute `sudo tedge connect c8y --test`
When we validate stdout
When we cleanup EnvironmentC8y
Then we find a successful message in stdout
Then the test has passed

"""


class RestartBridge(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        super().execute()
        self.log.info("Execute `tedge connect c8y --test`")
        self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y", "--test"],
            stdouterr="tedge_connect_c8y_--test_positive",
        )

    def validate(self):
        super().validate()
        self.log.info("Validate")
        self.assertGrep(
            "tedge_connect_c8y_--test_positive.out", "connection check is successful.", contains=True
        )

    def mycleanup(self):
        self.log.info("MyCleanup")
