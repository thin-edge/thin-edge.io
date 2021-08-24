import sys

sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Run connection test while being connected (positive case):

Given a configured system with configured certificate
When we setup EnvironmentC8y
When we execute `sudo tedge connect c8y`
When we validate stdout for Software Management Services started and enabled
When we cleanup EnvironmentC8y
Then we find a successful message in stdout
Then the test has passed

"""


class TedgeConnectTestSMServices(EnvironmentC8y):
    # `sudo tedge connect c8y` is run by the super.execute()
    def execute(self):
        super().execute()
      
    def validate(self):
        super().validate()
        self.log.info("Validate")
              
        # Validate if the Software management services are getting started and enabled properly on "tedge disconnect c8y"  
        # EnvironmentC8y captures the log messages in tedge_connect.out
        self.assertGrep(
            "tedge_connect.out", "tedge-agent service successfully started and enabled!", contains=True
        )
        
        self.assertGrep(
            "tedge_connect.out", "tedge-mapper-sm-c8y service successfully started and enabled!", contains=True
        )       
      