import pysys
from pysys.basetest import BaseTest

import time

"""
Validate local publishing and subscribing:

Given a configured system
When we start the bridge and the mapper
When we start tedge sub in the background
When we start tedge pub to publish a message
When we start tedge pub to publish another message
Then we find the messages in the output of tedge sub


Disabled due to this issue:

12:45:27 INFO  Contents of tedge_pub2.err:
12:45:27 INFO    Error: failed to read the tedge configuration
12:45:27 INFO    Caused by:
12:45:27 INFO        User's Home Directory not found.
"""

class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        sub = self.startProcess(
            command=tedge,
            arguments=[ "mqtt", "sub", "atopic"],
            stdouterr="tedge_sub",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize. This is a heuristic measure.
        # Without an additional wait we observe failures in 1% of the test
        # runs.
        time.sleep(0.1)

        pub = self.startProcess(
            command=tedge,
            arguments=[ "mqtt", "pub", "atopic", "amessage"],
            stdouterr="tedge_pub2",
        )

        pub = self.startProcess(
            command=tedge,
            arguments=[ "mqtt", "pub", "atopic", "the message"],
            stdouterr="tedge_pub3",
        )

    def validate(self):
        self.assertGrep("tedge_sub.out", "amessage", contains=True)
        self.assertGrep("tedge_sub.out", "the message", contains=True)

