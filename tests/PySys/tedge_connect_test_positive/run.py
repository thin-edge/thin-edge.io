import sys

sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Run connection test while being connected (positive case):

Given a configured system with configured certificate
When we setup EnvironmentC8y
When we execute `sudo tedge connect c8y --test`
When we validate stdout
When we cleanup EnvironmentC8y
Then we find a successful message in stdout
Then the test has passed

"""


class TedgeConnectTestPositive(EnvironmentC8y):
    def execute(self):
        super().execute()
        self.systemctl="/usr/bin/systemctl"
        self.log.info("Execute `tedge connect c8y --test`")
        self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y", "--test"],
            stdouterr="tedge_connect_c8y_test_positive",
        )

    def validate(self):
        super().validate()
        self.log.info("Validate")
        self.assertGrep(
            "tedge_connect_c8y_test_positive.out", "connection check is successful.", contains=True
        )
        self.assertGrep(
            "tedge_connect_c8y_test_positive.out", "tedge-agent service successfully started and enabled!", contains=True
        )
