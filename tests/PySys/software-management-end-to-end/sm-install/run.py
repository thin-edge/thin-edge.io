from pysys.basetest import BaseTest

import time

"""
Validate ...

"""

import base64
import json
import requests
import time


class PySysTest(BaseTest):


    def setup(self):
        tenant = self.project.tenant
        user = self.project.username
        password = self.project.c8ypass

        auth = bytes(f"{tenant}/{user}:{password}", "utf-8")
        self.header = {
            b"Authorization": b"Basic " + base64.b64encode(auth),
            b"content-type": b"application/json",
            b"Accept": b"application/json",
        }

    def trigger_action(self, package_name, package_id, version, url, action):

        url = "https://thin-edge-io.eu-latest.cumulocity.com/devicecontrol/operations"

        payload = {
            "deviceId": self.project.deviceid,
            "description": "Apply software changes: install rolldice",
            "c8y_SoftwareUpdate": [
                {
                    "id": package_id,
                    "name": package_name,
                    "version": version,
                    "url": url,
                    "action": action,
                                   }
            ],
        }

        req = requests.post(url, json=payload, headers=self.header)

        self.log.info(req)
        self.log.info(req.text)

    def is_status_success(self):
        # TODO Fix page size or just query todays operations
        url = "https://thin-edge-io.eu-latest.cumulocity.com/devicecontrol/operations?deviceId=4430276&pageSize=200&revert=true"
        req = requests.get(url, headers=self.header)
        j = json.loads(req.text)


        #for i in j["operations"]:
        i = j["operations"][-1]

        #self.log.info( i )
        self.log.info( i["status"] )
        # Observed states: PENDING, SUCCESSFUL, EXECUTING

        return i["status"] == "SUCCESSFUL"


    def check_isinstalled(self, package_name):

        url = f"https://thin-edge-io.eu-latest.cumulocity.com/inventory/managedObjects/{self.project.deviceid}"
        req = requests.get(url, headers=self.header)

        j = json.loads(req.text)
        ret = False
        for i in j["c8y_SoftwareList"]:
            if i["name"]== package_name:
                self.log.info( "It is installed" )
                self.log.info( i )
                ret = True
                break
        return ret

    def execute(self):

        if self.myPlatform != 'container':
            self.skipTest('Testing the apt plugin is not supported on this platform')

        self.trigger_action("rolldice", "5445239", "::apt", "notanurl", "install")
        #self.trigger_action("rolldice", "5445239", "::apt", "notanurl", "delete")

        while True:
            if self.is_status_success():
                break
            time.sleep(1)

    def validate(self):

        self.assertThat("True == value", value=self.check_isinstalled("rolldice"))


