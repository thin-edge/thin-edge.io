import sys

sys.path.append("environments")
from environment_roundtrip_c8y import Environment_roundtrip_c8y

"""
Roundtrip test C8y 400 samples 1Xms delay

Given a configured system with configured certificate
When we derive from EnvironmentC8y
When we run the smoketest for REST publishing with defaults a size of 400, 10ms delay
Then we validate the data from C8y

X=15ms : Works good
#X=10ms : We observe dataloss ocationally
TODO : Further investigation necessary
"""


class SmoketestSmartRest400Samples1Xms(Environment_roundtrip_c8y):
    def setup(self):
        super().setup()
        self.samples = "400"
        self.delay = "15"
        self.timeslot = "60"  # Temporarily increased to run at mythic beasts
        self.style = "REST"
