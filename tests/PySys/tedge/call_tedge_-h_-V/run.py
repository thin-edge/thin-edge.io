import pysys
from pysys.basetest import BaseTest

import time

"""
Validate command line option -h -V

Given a running system
When we call tedge -h -V
Then we find the version string as well in the output
Then we find the string USAGE: in the output

Note: This is a candidate for deletion or transference to Rust.
In the past, there was a async issue with -h and -V this is wy whe created
some tests that do very similar things. They are probably not needed anymore.
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"

        proc = self.startProcess(
            command=tedge,
            arguments=["-h", "-V"],
            stdouterr="tedge",
            expectedExitStatus="==0",
        )

    def validate(self):
        self.assertGrep("tedge.out", "tedge 0.6", contains=True)
        self.assertGrep("tedge.out", "USAGE:", contains=True)
