import sys

sys.path.append("environments")
from environment_roundtrip_c8y import Environment_roundtrip_c8y

"""
Roundtrip test C8y 400 samples 20ms delay

Given a configured system with configured certificate
When we derive from EnvironmentC8y
When we run the smoketest for REST publishing with defaults a size of 400, 20ms delay
Then we validate the data from C8y
"""


class SmoketestSmartRest400Samples20ms(Environment_roundtrip_c8y):
    def setup(self):
        super().setup()
        self.samples = "200"
        self.delay = "20"
        self.timeslot = "40"  # Temporarily increased to run at mythic beasts
        self.style = "REST"
