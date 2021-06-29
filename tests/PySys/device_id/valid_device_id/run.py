from pysys.basetest import BaseTest

import time

"""
Validate certificate creation with valid characters and validate with cumulocity cloud

Given a configured system
When we create a certificate using the valid characters using the sudo tedge
When we upload the certificate onto the c8y cloud
When we connect to the c8y cloud
Then we check for the output of the tedge connect and find successfull
"""


class PySysTest(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        c8y_connect = self.startProcess(
            c8y_connect=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="c8y_connect",
        )

        self.addCleanupFunction(self.device_id_cleanup)

    def execute(self):
        sub1 = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "cert", "create",
                       "--device-id", "'?=()*@!%,-.123ThinEdgeDevice-id"],
            stdouterr="cert_create",
        )

        c8y_connect = self.startProcess(
            cert_upload=self.sudo,
            arguments=[self.tedge, "cert", "upload",
                       "c8y", "--user", "pradeep"],
            stdouterr="cert_upload",
        )

        c8y_connect = self.startProcess(
            c8y_connect=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="c8y_connect",
        )

    def validate(self):
        self.assertGrep("c8y_connect.out", "Successfull", contains=True)

    def device_id_cleanup(self):
        self.log.info("monitoring_cleanup")
        c8y_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="c8y_disconnect",
        )
