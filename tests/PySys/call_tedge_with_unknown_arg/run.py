import pysys
from pysys.basetest import BaseTest

import time

"""
Validate tedge with an unknown command line option

Given a running system
When we call tedge with an unknown command line parameter
Then we get exit status 1
Then we find an error string in stderr
Then we find an usage info in stderr
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
        self.assertGrep("tedge.err", "which wasn't expected, or isn't valid in this context", contains=True)
        self.assertGrep("tedge.err", "USAGE:", contains=True)
