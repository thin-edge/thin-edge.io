from pysys.basetest import BaseTest

import time

"""
Validate certificate creation with valid characters and validate with cumulocity cloud

Given a configured system
When a temporary directory is created to store the certificate and key
When we create a certificate using the valid characters using the sudo tedge
When we upload the certificate onto the c8y cloud
When we connect to the c8y cloud
Then we check for the output of the tedge connect and find successfull
Cleanup the Certificate and Key path and delete the temporary directory
"""


class ValidateValidDeviceId(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # disconnect the device from cloud
        c8y_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="c8y_disconnect",
        )

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
            stdouterr="change_owner_of_dir",
        )

        # set the custom certificate path
        set_cert_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "device.cert.path",
                       "/tmp/test-device-certs/tedge-certificate.pem"],
            stdouterr="set_cert_path",
        )

        # set the custom key path
        set_key_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "device.key.path",
                       "/tmp/test-device-certs/tedge-private-key.pem"],
            stdouterr="set_key_path",
        )

        self.addCleanupFunction(self.device_id_cleanup)

    def execute(self):
        # create a certificate with a device id that contains all valid characters
        sub1 = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "cert", "create",
                       "--device-id", "'?=()*@!%,-.123ThinEdgeDevice-id"],
            stdouterr="cert_create",
        )

        # upload the certificate
        cert_upload = self.startProcess(
            environs={"C8YPASS":self.project.c8ypass},
            command=self.sudo,
            arguments=["-E", self.tedge, "cert", "upload",
                       "c8y", "--user", self.project.c8yusername],
            stdouterr="cert_upload",
        )

        time.sleep(1)

        # connect to the c8y cloud
        c8y_connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="c8y_connect",
        )

        # test connect to the c8y cloud
        c8y_connect_test = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y", "--test"],
            stdouterr="c8y_connect_test",
        )

    def validate(self):
        # validate the connection is successfull
        self.assertGrep("c8y_connect.out", "successful", contains=True)
        self.assertGrep("c8y_connect_test.out", "successful", contains=True)

    def device_id_cleanup(self):

        # disconnect the test
        c8y_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="c8y_disconnect",
        )

        # unset the device certificate path
        unset_cert_path = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "device.cert.path"],
            stdouterr="unset_cert_path",
        )

        # unset the device key path
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
