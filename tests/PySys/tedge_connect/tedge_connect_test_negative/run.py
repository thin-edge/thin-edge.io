from environment_tedge import TedgeEnvironment

"""
Run connection test without being connected (negative case):

Given a configured system
When we execute `sudo tedge connect c8y --test`
When we validate stderr
Then we find an error message in stderr
Then test has passed
"""


class TedgeConnectTestNegative(TedgeEnvironment):
    def execute(self):

        self.log.info("Execute `tedge connect c8y --test`")
        self.tedge_connect_c8y_test(expectedExitStatus="==1")

    def validate(self):
        self.log.info("Validate")
        fail = "Error: failed to test connection to Cumulocity cloud."
        self.assertGrep("tedge_connect_c8y_test.err", fail, contains=True)
