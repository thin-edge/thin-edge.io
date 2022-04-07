import pysys
from pysys.basetest import BaseTest

import time
import os

"""
Validate local publishing and subscribing:

Given a configured system
When we start the bridge and the mapper
When we start sudo tedge sub in the background
When we start sudo tedge pub to publish a message
When we start sudo tedge pub to publish another message
Then we find the messages in the output of tedge sub
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        sub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "sub", "atopic"],
            stdouterr="tedge_sub",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize. This is a heuristic measure.
        # Without an additional wait we observe failures in 1% of the test
        # runs.
        time.sleep(0.1)

        pub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "pub", "atopic", "amessage"],
            stdouterr="tedge_pub2",
        )

        pub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "pub", "atopic", "the message"],
            stdouterr="tedge_pub3",
        )

        # wait for a while before killing the subscribers
        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertGrep("tedge_sub.out", "amessage", contains=True)
        self.assertGrep("tedge_sub.out", "the message", contains=True)
