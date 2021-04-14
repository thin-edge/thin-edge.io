import pysys
from pysys.basetest import BaseTest

import time

"""
Validate command line option -h -V

Given a running system
When we call tedge -h -V
Then we find the string USAGE: in the output of stderr

Note: This should probably return a non zero exit status and complain
See also: https://cumulocity.atlassian.net/browse/CIT-318
"""


class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"

        proc = self.startProcess(
            command=tedge,
            arguments=["-h"],
            arguments=["-V"],
            stdouterr="tedge",
            expectedExitStatus="==1",
        )

    def validate(self):
        self.assertGrep("tedge.err", "USAGE:", contains=True)
