from pysys.basetest import BaseTest

import time

"""
Validate ...

"""

import base64
import requests

class PySysTest(BaseTest):
    def execute(self):

        if self.myPlatform != 'container':
            self.skipTest('Testing the apt plugin is not supported on this platform')

        tenant = self.project.tenant
        user = self.project.username
        password = self.project.c8ypass

        url = "https://thin-edge-io.eu-latest.cumulocity.com/devicecontrol/operations"

        payload = {
            "deviceId": self.project.deviceid,
            "description": "Apply software changes: install rolldice",
            "c8y_SoftwareUpdate": [
                {
                    "id": "5445239",
                    "name": "rolldice",
                    "version": "::apt",
                    "url": "notanurl",
                    "action": "install",
                }
            ],
        }

        auth = bytes(f"{tenant}/{user}:{password}", "utf-8")
        header = {
            b"Authorization": b"Basic " + base64.b64encode(auth),
            b"content-type": b"application/json",
            b"Accept": b"application/json",
        }

        req = requests.post(url, json=payload, headers=header)



        self.log.info(req)
        self.log.info(req.text)

    def validate(self):
        pass
