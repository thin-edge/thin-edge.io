from pysys.basetest import BaseTest

import time

"""
Validate cert create with invalid characters

Given a configured system
When we create a certificate with a invalid/non-supported character
Then we find the error messages in the output of tedge cert create
"""


class PySysTest(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

    def execute(self):
        cert_create = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "cert", "create",
                       "--device-id", "':?=()*@!%,-.123ThinEdgeDevice-id"],
            stdouterr="cert_create",           
        )

    def validate(self):
        self.assertGrep("cert_create.out", "invalid", contains=True)
        
