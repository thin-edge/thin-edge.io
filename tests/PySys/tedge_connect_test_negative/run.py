from pysys.basetest import BaseTest

"""
Run connection test without being connected (negative case):

Given a configured system
When we execute `sudo tedge connect c8y --test`
When we validate stderr
Then we find an error message in stderr
Then test has passed
"""


class TedgeConnectTestNegative(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        self.log.info("Execute `tedge connect c8y --test`")
        self.startProcess(
            command=sudo,
            arguments=[tedge, "connect", "c8y", "--test"],
            stdouterr="tedge_connect_c8y_test_negative",
            expectedExitStatus="==1",
        )

    def validate(self):
        self.log.info("Validate")
        fail = "Error: failed to test connection to Cumulocity cloud."
        self.assertGrep("tedge_connect_c8y_test_negative.err", fail, contains=True)

