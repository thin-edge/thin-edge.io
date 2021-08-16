from pysys.basetest import BaseTest

import time

"""
Validate ...

"""

import base64
import json
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
                    #"action": "delete",
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

        url = f"https://thin-edge-io.eu-latest.cumulocity.com/inventory/managedObjects/{self.project.deviceid}"
        req = requests.get(url, headers=header)

        #self.log.info(req)
        #self.log.info(req.text)

        j = json.loads(req.text)

        for i in j["c8y_SoftwareList"]:
            if i["name"]=="rolldice":
                self.log.info( "It is installed" )
                self.log.info( i )
                break



    def validate(self):
        pass
