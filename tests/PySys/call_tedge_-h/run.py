import pysys
from pysys.basetest import BaseTest

import time

"""
Validate command line option -h
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"

        proc = self.startProcess(
            command=tedge,
            arguments=["-h"],
            stdouterr="tedge",
            expectedExitStatus='==0',
        )

    def validate(self):
        self.assertGrep("tedge.out", "USAGE:", contains=True)
        self.assertGrep("tedge.out", "FLAGS:", contains=True)
        self.assertGrep("tedge.out", "SUBCOMMANDS:", contains=True)
