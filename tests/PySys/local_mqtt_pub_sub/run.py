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
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        sub1 = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "sub", "atopic"],
            stdouterr="tedge_sub",
            background=True,
        )

        sub2 = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "sub", "--no-topic", "atopic"],
            stdouterr="tedge_sub_no_topic",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize
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

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=sudo,
            arguments=["kill", "-9", str(sub1.pid)],
            stdouterr="kill_out",
        )

        kill = self.startProcess(
            command=sudo,
            arguments=["kill", "-9", str(sub2.pid)],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertGrep("tedge_sub.out", "\[atopic\] amessage", contains=True)
        self.assertGrep("tedge_sub.out", "\[atopic\] the message", contains=True)
        self.assertGrep("tedge_sub_no_topic.out", "atopic", contains=False)
