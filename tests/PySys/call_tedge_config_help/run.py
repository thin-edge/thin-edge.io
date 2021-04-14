import pysys
from pysys.basetest import BaseTest

import time

"""
Validate command line option help

Given a running system
When we call tedge help
Then we find the string USAGE: in the output
Then we find the string FLAGS: in the output
Then we find the string SUBCOMMANDS: in the output
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"

        proc = self.startProcess(
            command=tedge,
            arguments=["config", "help"],
            stdouterr="tedge",
            expectedExitStatus="==0",
        )

    def validate(self):
        self.assertGrep("tedge.out", "USAGE:", contains=True)
        self.assertGrep("tedge.out", "FLAGS:", contains=True)
        self.assertGrep("tedge.out", "SUBCOMMANDS:", contains=True)
