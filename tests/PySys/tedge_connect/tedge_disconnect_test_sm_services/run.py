import sys

sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Run connection test while being connected :

Given a configured system with configured certificate
When we setup EnvironmentC8y
When we execute `sudo tedge connect c8y`
When we execute `sudo tedge disconnect c8y`
When we validate stdout for Software Management Services stopped and disabled
When we cleanup EnvironmentC8y
Then the test has passed

"""

class TedgeDisConnectTestSMServices(EnvironmentC8y):
    # The base class rexecutes the `sudo tedge connect c8y`
    def validate(self):
        super().validate()

        self.tedge_disconnect_c8y()

        # Validate if the Software management services are getting stopped and disabled properly on "tedge disconnect c8y"
        self.assertGrep(
            "tedge_disconnect_c8y.out", "tedge-agent service successfully stopped and disabled!", contains=True
        )

        self.assertGrep(
            "tedge_disconnect_c8y.out", "tedge-mapper-sm-c8y service successfully stopped and disabled!", contains=True
        )
