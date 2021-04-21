import sys

sys.path.append("environments")
from environment_roundtrip_c8y import Environment_roundtrip_c8y

"""
Roundtrip test C8y via SmartREST

Given a configured system with configured certificate
When we derive from EnvironmentC8y
When we run the smoketest for REST publishing with defaults a size of 20, 100ms delay
Then we validate the data from C8y

20 samples 100ms delay
"""


class SmoketestSmartRest(Environment_roundtrip_c8y):
    def setup(self):
        super().setup()
        self.samples = "20"
        self.delay = "100"
        self.timeslot = "10"
        self.style = "REST"
