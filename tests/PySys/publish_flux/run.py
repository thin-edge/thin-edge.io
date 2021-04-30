import os
import sys

sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Publish to flux topic

Given a configured system with configured certificate
When we derive from EnvironmentC8y
When we publish with the sawtooth_publisher with 100ms cycle time and publish
    5 times 100 values to the Flux topic.
Then we manually observe the data in C8y

TODO : Add validation procedure
"""


class PublishFlux(EnvironmentC8y):
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
            command=cmd, arguments=["100", "100", "5", "flux"], stdouterr="stdout"
        )

    def validate(self):
        super().validate()
        self.log.info("Validate - Do it")

    def mycleanup(self):
        self.log.info("My Cleanup")
