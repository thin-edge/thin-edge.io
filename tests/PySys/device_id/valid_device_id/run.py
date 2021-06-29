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

        c8y_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="c8y_disconnect",
        )

        create_cert_dir = self.startProcess(
            command=self.sudo,
            arguments=["mkdir test-device-certs"],
            stdouterr="create_cert_dir",
        )

        set_cert_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "device.cert.path",
                       "/tmp/test-device-certs/tedge-certificate.pem"],
            stdouterr="set_cert_path",
        )

        set_key_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "device.key.path",
                       "/tmp/test-device-certs/tedge-private-key.pem"],
            stdouterr="set_key_path",
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

        unset_cert_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "device.cert.path"],
            stdouterr="unset_cert_path",
        )

        unset_key_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "device.key.path"],
            stdouterr="unset_key_path",
        )

        remove_cert_dir = self.startProcess(
            command=self.sudo,
            arguments=["rm -rf test-device-certs"],
            stdouterr="remove_cert_dir",
        )
