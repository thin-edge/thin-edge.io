from pysys.basetest import BaseTest

import time

"""
Validate cert create with invalid characters

Given a configured system
When a temporary directory is created to store the certificate and key
When we create a certificate with a invalid/non-supported character
Then we find the error messages in the output of tedge cert create
Cleanup the Certificate and Key path and delete the temporary directory
"""


class InvalidDeviceId(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # create a custom certiticate directory for testing purpose
        create_cert_dir = self.startProcess(
            command=self.sudo,
            arguments=["mkdir", "/tmp/test-device-certs"],
            stdouterr="create_cert_dir",
        )

        # change the ownership of the directory
        change_owner_of_dir = self.startProcess(
            command=self.sudo,
            arguments=["chown", "mosquitto:mosquitto", "/tmp/test-device-certs"],
            stdouterr="create_cert_dir",
        )

        # set the custom certificate path
        set_cert_path = self.startProcess(
            command=self.sudo,
            arguments=[
                self.tedge,
                "config",
                "set",
                "device.cert.path",
                "/tmp/test-device-certs/tedge-certificate.pem",
            ],
            stdouterr="set_cert_path",
        )

        # set the custom key path
        set_key_path = self.startProcess(
            command=self.sudo,
            arguments=[
                self.tedge,
                "config",
                "set",
                "device.key.path",
                "/tmp/test-device-certs/tedge-private-key.pem",
            ],
            stdouterr="set_key_path",
        )

        self.addCleanupFunction(self.device_id_cleanup)

    def execute(self):
        # create a certificate with a device id that contains a  invalid character
        cert_create = self.startProcess(
            command=self.sudo,
            arguments=[
                self.tedge,
                "cert",
                "create",
                "--device-id",
                "':?=()*@!%,-.123ThinEdgeDevice-id",
            ],
            stdouterr="cert_create",
            expectedExitStatus="==1",
        )

    def validate(self):
        # check for the error
        self.assertGrep("cert_create.err", "DeviceID Error", contains=True)

    def device_id_cleanup(self):
        # unset the custom device.cert.path
        unset_cert_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "device.cert.path"],
            stdouterr="unset_cert_path",
        )

        # unset the custom device.key.path
        unset_key_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "device.key.path"],
            stdouterr="unset_key_path",
        )

        # delete the temporary directory
        remove_cert_dir = self.startProcess(
            command=self.sudo,
            arguments=["rm", "-rf", "/tmp/test-device-certs"],
            stdouterr="remove_cert_dir",
        )
