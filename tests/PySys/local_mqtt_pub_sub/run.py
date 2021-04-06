import pysys
from pysys.basetest import BaseTest

import time

"""
Validate local publishing and subscribing:

Given a configured system
When we start the bridge and the mapper
When we start tedge sub with sudo in the background
When we start tedge pub with sudo to publish a message
When we start tedge pub with sudo to publish another message
Then we find the messages in the output of tedge sub
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        sub = self.startProcess(
            command = sudo,
            arguments=[tedge, "mqtt", "sub", "atopic"],
            stdouterr="tedge_sub",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize
        time.sleep(0.1)

        pub = self.startProcess(
            command = sudo,
            arguments=[tedge, "mqtt", "pub", "atopic", "amessage"],
            stdouterr="tedge_pub2",
        )

        pub = self.startProcess(
            command= sudo,
            arguments=[tedge, "mqtt", "pub", "atopic", "the message"],
            stdouterr="tedge_pub3",
        )
        pub = self.startProcess(
            command= sudo,
            arguments=["kill", "-9", str(sub.pid)],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertGrep("tedge_sub.out", "amessage", contains=True)
        self.assertGrep("tedge_sub.out", "the message", contains=True)

