from pysys.basetest import BaseTest

import time

from environment_tedge import TedgeEnvironment

"""
Validate retrieved JWT Token from cumulocity cloud

Given a configured system
When the thin edge device connected to the c8y cloud successfully
When the empty message was published on `c8y/s/uat` topic
When the JWT Token was received as response on the topic 'c8y/s/dat'
Verify the response that starts with '71'
Cleanup the Certificate and Key path and delete the temporary directory
"""


class ValidateJWTTokenRetrieval(TedgeEnvironment):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # disconnect the device from cloud
        self.tedge_disconnect_c8y()

        self.addCleanupFunction(self.jwt_token_cleanup)

    def execute(self):
        # connect the device to cloud
        self.tedge_connect_c8y()

        # Subscribe for the jwt token response topic
        resp_jwt_token = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "c8y/s/dat"],
            stdouterr="resp_jwt",
            background=True,
        )

        # Publish an empty message on jwt token request topic
        req_jwt_token = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "c8y/s/uat", "''"],
            stdouterr="req_jwt",
        )

        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

    def validate(self):
        # validate the correct response/token is received
        self.assertGrep("resp_jwt.out", "\[c8y/s/dat\] 71,", contains=True)

    def jwt_token_cleanup(self):
        # disconnect the test
        self.tedge_disconnect_c8y()
