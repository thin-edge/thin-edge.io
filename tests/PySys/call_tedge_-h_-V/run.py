import pysys
from pysys.basetest import BaseTest

import time

"""
Validate command line option -h -V
This should return a non zero exit status and complain
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"

        proc = self.startProcess(
            command=tedge,
            arguments=["-h"],
            arguments=["-V"],
            stdouterr="tedge",
            expectedExitStatus='==1',
        )

    def validate(self):
        self.assertGrep("tedge.err", "USAGE:", contains=True)
