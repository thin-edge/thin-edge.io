import base64
import time
from datetime import datetime, timedelta, timezone

import json
import requests

import pysys
from pysys.basetest import BaseTest

def is_timezone_aware(stamp):
    """determine if object is timezone aware or naive
    See also: https://docs.python.org/3/library/datetime.html?highlight=tzinfo#determining-if-an-object-is-aware-or-naive
    """
    return stamp.tzinfo is not None and stamp.tzinfo.utcoffset(stamp) is not None


class SmManagement(BaseTest):

    PAGE_SIZE = "500"

    def setup(self):

        if self.myPlatform != "container":
            self.skipTest("Testing the apt plugin is not supported on this platform")

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
            "description": "Apply software changes, triggered from PySys test",
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

        timeslot = 600
        time_to = datetime.now(timezone.utc).replace(microsecond=0)
        time_from = time_to - timedelta(seconds=timeslot)

        assert is_timezone_aware(time_from)

        date_from = time_from.isoformat(sep="T")
        date_to = time_to.isoformat(sep="T")

        params = {
            "deviceId": self.project.deviceid,
            "pageSize": self.PAGE_SIZE,
            "dateFrom": date_from,
            "dateTo": date_to,
            "revert": "true",
        }

        url = "https://thin-edge-io.eu-latest.cumulocity.com/devicecontrol/operations"
        req = requests.get(url, params=params, headers=self.header)
        j = json.loads(req.text)

        if not j["operations"]:
            self.log.error("No operations found")
            return None
        i = j["operations"][0]

        self.log.info(i["status"])
        # Observed states: PENDING, SUCCESSFUL, EXECUTING

        return i["status"] == "SUCCESSFUL"

    def wait_until_succcess(self):
        """Wait until c8y reports a success
        TODO This might block forever
        """

        # wait for some time to let c8y process a request until we can poll for it
        time.sleep(1)

        while True:
            if self.is_status_success():
                break
            time.sleep(1)

    def check_isinstalled(self, package_name):

        url = f"https://thin-edge-io.eu-latest.cumulocity.com/inventory/managedObjects/{self.project.deviceid}"
        req = requests.get(url, headers=self.header)

        j = json.loads(req.text)

        ret = False
        for i in j["c8y_SoftwareList"]:
            if i["name"] == package_name:
                self.log.info("It is installed")
                self.log.info(i)
                ret = True
                break
        return ret

