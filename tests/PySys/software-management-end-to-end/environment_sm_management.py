import base64
import time
from datetime import datetime, timedelta, timezone

import json
import requests

import pysys
from pysys.basetest import BaseTest


"""
This environment provides an interface to software management features
through the C8y REST API.
With these we can emulate a user doing operations in the C8y UI.
They are rather slow as they use the complete chain from end to end.

WARNING: Handle with care!!!
The C8YDEVICEID will handle on which this test will install and remove packages.
"""


def is_timezone_aware(stamp):
    """determine if object is timezone aware or naive
    See also: https://docs.python.org/3/library/datetime.html?highlight=tzinfo#determining-if-an-object-is-aware-or-naive
    """
    return stamp.tzinfo is not None and stamp.tzinfo.utcoffset(stamp) is not None


class SmManagement(BaseTest):

    PAGE_SIZE = "500"

    def setup(self):
        """Setup Environment"""

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
        """Trigger a installation or deinstallation of a package"""

        self.trigger_action_json(
            [
                {
                    "id": package_id,
                    "name": package_name,
                    "version": version,
                    "url": url,
                    "action": action,
                }
            ]
        )

    def trigger_action_json(self, json):
        """Take an actions description that is then forwarded to c8y"""

        url = "https://thin-edge-io.eu-latest.cumulocity.com/devicecontrol/operations"

        payload = {
            "deviceId": self.project.deviceid,
            "description": "Apply software changes, triggered from PySys test",
            "c8y_SoftwareUpdate": json,
        }

        req = requests.post(url, json=payload, headers=self.header)

        self.log.info(f"Response: {req}")
        self.log.info(f"Response to action: {req.text}")

        if req.status_code != 201:  # Request was accepted
            raise SystemError("Got HTTP status %s", req.status_code)

    def is_status_fail(self):
        return self.is_status("FAILED")

    def is_status_success(self):
        return self.is_status("SUCCESSFUL")

    def is_status(self, status):
        """Check if the last operation is successfull"""

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

        if req.status_code != 200:  # Request was accepted
            raise SystemError("Got HTTP status %s", req.status_code)

        jresponse = json.loads(req.text)

        if not jresponse["operations"]:
            self.log.error("No operations found")
            return None
        operation = jresponse["operations"][0]

        self.log.info(operation["status"])
        # Observed states: PENDING, SUCCESSFUL, EXECUTING, FAILED

        return operation["status"] == status

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

    def wait_until_fail(self):
        """Wait until c8y reports a success
        TODO This might block forever
        """

        # wait for some time to let c8y process a request until we can poll for it
        time.sleep(1)

        while True:
            if self.is_status_fail():
                break
            time.sleep(1)

    def check_isinstalled(self, package_name, version=None):
        """Check if a package is installed"""

        url = f"https://thin-edge-io.eu-latest.cumulocity.com/inventory/managedObjects/{self.project.deviceid}"
        req = requests.get(url, headers=self.header)

        if req.status_code != 200:
            raise SystemError("Got HTTP status %s", req.status_code)

        jresponse = json.loads(req.text)

        ret = False
        for package in jresponse["c8y_SoftwareList"]:
            if package["name"] == package_name:
                self.log.info(f"Package {package_name} is installed")
                # self.log.info(package)
                if version:
                    if package["version"]==version:
                        ret = True
                        break
                    else:
                        raise SystemError("Wrong version is installed")
                else:
                    ret = True
                    break
        return ret
