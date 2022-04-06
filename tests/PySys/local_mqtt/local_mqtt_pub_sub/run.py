from pysys.basetest import BaseTest

import time
import os

"""
Validate local publishing and subscribing:

Given a configured system
When we start tedge sub with sudo in the background
When we start tedge pub with sudo to publish a message
When we start tedge pub with sudo to publish another message
Then we kill tedge sub with sudo as it is running with a different user account
Then we find the messages in the output of tedge sub
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"
        environ = {"HOME": os.environ.get("HOME")}

        sub1 = self.startProcess(
            command=tedge,
            arguments=["mqtt", "sub", "atopic"],
            stdouterr="tedge_sub",
            background=True,
            environs=environ,
        )

        sub2 = self.startProcess(
            command=tedge,
            arguments=["mqtt", "sub", "--no-topic", "atopic"],
            stdouterr="tedge_sub_no_topic",
            background=True,
            environs=environ,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize. This is a heuristic measure.
        # Without an additional wait we observe failures in 1% of the test
        # runs.
        time.sleep(0.1)

        pub = self.startProcess(
            command=tedge,
            arguments=["mqtt", "pub", "atopic", "amessage"],
            stdouterr="tedge_pub2",
            environs=environ,
        )

        pub = self.startProcess(
            command=tedge,
            arguments=["mqtt", "pub", "atopic", "the message"],
            stdouterr="tedge_pub3",
            environs=environ,
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
