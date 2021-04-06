import pysys
from pysys.basetest import BaseTest

import time

"""
Validate tedge with an unknown command line option
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"

        proc = self.startProcess(
            command=tedge,
            arguments=["nope"],
            stdouterr="tedge",
            expectedExitStatus='==1',
        )

    def validate(self):
        self.assertGrep("tedge.err", "error: Found argument", contains=True)
        self.assertGrep("tedge.err", "USAGE:", contains=True)
