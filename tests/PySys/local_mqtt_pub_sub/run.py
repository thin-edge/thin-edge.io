import pysys
from pysys.constants import *
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

        sub = self.startProcess(
            command=tedge,
            arguments=["mqtt", "sub", "atopic"],
            stdouterr="tedge_sub",
            background=True,
        )

        time.sleep(0.1)

        pub = self.startProcess(
            command=tedge,
            arguments=["mqtt", "pub", "atopic", "amessage"],
            stdouterr="tedge_pub",
        )

        pub = self.startProcess(
            command=tedge,
            arguments=["mqtt", "pub", "atopic", "the message"],
            stdouterr="tedge_pub",
        )

    def validate(self):
        self.assertGrep("tedge_sub.out", "amessage", contains=True)
        self.assertGrep("tedge_sub.out", "the message", contains=True)
