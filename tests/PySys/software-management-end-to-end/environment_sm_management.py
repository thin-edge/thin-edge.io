"""
This environment provides an interface to software management features
through the C8y REST API.
With these we can emulate a user doing operations in the C8y UI.
They are rather slow as they use the complete chain from end to end.

WARNING: Handle with care!!!
The C8YDEVICEID will handle on which device this test will install and remove packages.

These tests are disabled by default as they will install and deinstall packages.
Better run them in a VM or a container.

To run the tests:

    pysys.py run 'sm-apt*' -XmyPlatform='specialcontainer'

To run the tests with another tenant url:

    pysys.py run 'sm-apt*' -XmyPlatform='specialcontainer' -Xtenant_url='thin-edge-io.eu-latest.cumulocity.com'



TODO: Avoid hardcoded ids
TODO: Get software package ids from c8y
TODO: Add management for package creation and removal for c8y
    -> Mabe as separte python module to access c8y


"""

import base64
import time
import json
import platform
import requests
import subprocess
import sys

import pysys
from pysys.basetest import BaseTest

sys.path.append("./environments")
from environment_c8y import EnvironmentC8y


def is_timezone_aware(stamp):
    """determine if object is timezone aware or naive
    See also: https://docs.python.org/3/library/datetime.html?highlight=tzinfo#determining-if-an-object-is-aware-or-naive
    """
    return stamp.tzinfo is not None and stamp.tzinfo.utcoffset(stamp) is not None


class SoftwareManagement(EnvironmentC8y):
    """Base class for software management tests"""

    # Static class member that can be overriden by a command line argument
    # E.g.:
    # pysys.py run 'sm-apt*' -XmyPlatform='specialcontainer'

    myPlatform = None

    # Static class member that can be overriden by a command line argument
    # E.g.:
    # pysys.py run 'sm-fake*' -Xfakeplugin='fakeplugin'
    # Use it only when you have set up the dummy_plugin to install fruits

    fakeplugin = None

    tenant_url = "thin-edge-io.eu-latest.cumulocity.com"

    def setup(self):
        """Setup Environment"""

        if self.myPlatform != "specialcontainer":
            self.skipTest("Testing the apt plugin is not supported on this platform")

        super().setup()
        self.addCleanupFunction(self.mysmcleanup)

        tenant = self.project.tenant
        user = self.project.username
        password = self.project.c8ypass

        # TODO are we doing something wrong while requesting?
        self.timeout_req = 80  # seconds, got timeout with 60s

        # Place to save the id of the operation that we started.
        # This is suitable for one operation and not for multiple ones running
        # at the same time.
        self.operation_id = None

        auth = bytes(f"{tenant}/{user}:{password}", "utf-8")
        self.header = {
            b"Authorization": b"Basic " + base64.b64encode(auth),
            b"content-type": b"application/json",
            b"Accept": b"application/json",
        }

        # Make sure we have no last operations pending or executing
        self.wait_until_end()

    def trigger_action(self, package_name, package_id, version, url, action):
        """Trigger a installation or deinstallation of a package.
        package_id is the id that is automatically assigned by C8y.

        TODO Improve repository ID management to avoid hardcoded IDs
        """

        self.trigger_action_json(
            [
                {
                    "name": package_name,
                    "id": package_id,
                    "version": version,
                    "url": url,
                    "action": action,
                }
            ]
        )

    def trigger_action_json(self, json_content):
        """Take an actions description that is then forwarded to c8y.
        So far, no checks are done on the json_content.

        TODO Improve repository ID management to avoid hardcoded IDs
        """

        url = f"https://{self.tenant_url}/devicecontrol/operations"

        payload = {
            "deviceId": self.project.deviceid,
            "description": f"Apply software changes, triggered from PySys: {json_content}",
            "c8y_SoftwareUpdate": json_content,
        }

        req = requests.post(
            url, json=payload, headers=self.header, timeout=self.timeout_req
        )

        jresponse = json.loads(req.text)

        self.log.info("Response status: %s", req.status_code)
        self.log.info("Response to action: %s", json.dumps(jresponse, indent=4))

        self.operation = jresponse
        self.operation_id = jresponse.get("id")

        if not self.operation_id:
            raise SystemError("field id is missing in response")

        self.log.info("Started operation: %s", self.operation)

        req.raise_for_status()

    def is_status_fail(self):
        """Check if the current status is a fail"""
        if self.operation_id:
            return self.check_status_of_operation("FAILED")
        return self.check_status_of_last_operation("FAILED")

    def is_status_success(self):
        """Check if the current status is a success"""
        if self.operation_id:
            return self.check_status_of_operation("SUCCESSFUL")
        return self.check_status_of_last_operation("SUCCESSFUL")

    def get_status_of_last_operation(self):

        params = {
            "deviceId": self.project.deviceid,
            "pageSize": 1,
            # To get the latest records first
            "revert": "true",
            # By using the date we make sure that the request comes
            # sorted, otherwise the revert does not seem to have an
            # effect. The lower boundary seems to be ok so we just
            # use the beginning of the epoch same as the c8y ui.
            "dateFrom": "1970-01-01T00:00:00.000Z",
        }

        url = f"https://{self.tenant_url}/devicecontrol/operations"
        req = requests.get(
            url, params=params, headers=self.header, timeout=self.timeout_req
        )

        req.raise_for_status()

        self.log.debug("Final URL of the request: %s", req.url)

        jresponse = json.loads(req.text)

        if not jresponse["operations"]:
            # This can happen e.g. after a weekend when C8y deleted the operations
            self.log.error("No operations found, assuming it passed")
            return True

        # Get the last operation, when we set "revert": "true" we can read it
        # from the beginning of the list

        operations = jresponse.get("operations")

        if not operations or len(operations) != 1:
            raise SystemError("field operations is mising in response or to long")

        operation = operations[0]

        # Observed states: PENDING, SUCCESSFUL, EXECUTING, FAILED
        self.log.info("State of current operation: %s", operation.get("status"))

        # In this case we just jump everything to see what is goin on
        if operation.get("status") in ["FAILED", "PENDING"]:
            self.log.debug("Final URL of the request: %s", req.url)
            self.log.debug(
                "State of current operation: %s", json.dumps(operation, indent=4)
            )

        if not operation.get("status"):
            raise SystemError("No valid field status in response")

        return operation.get("status")

    def check_status_of_last_operation(self, status):
        """Check if the last operation is successfull.
        Warning: an observation so far is, that installation failures
        seem to be at the beginning of the list independent of if we
        revert it or not.
        """

        current_status = self.get_status_of_last_operation()

        return current_status == status

    def get_status_of_operation(self):
        """Get the last operation"""

        if not self.operation_id:
            raise SystemError("No valid operation ID available")

        url = f"https://{self.tenant_url}/devicecontrol/operations/{self.operation_id}"
        req = requests.get(url, headers=self.header, timeout=self.timeout_req)

        req.raise_for_status()

        operation = json.loads(req.text)

        # Observed states: PENDING, SUCCESSFUL, EXECUTING, FAILED
        self.log.info(
            "State of operation %s : %s", self.operation_id, operation["status"]
        )

        if not operation.get("status"):
            raise SystemError("No valid field status in response")

        return operation.get("status")

    def check_status_of_operation(self, status):
        """Check if the last operation is successfull"""
        current_status = self.get_status_of_operation()
        self.log.info("Expected status: %s, got status %s" % (status, current_status))
        return current_status == status

    def wait_until_succcess(self):
        """Wait until c8y reports a success"""
        self.wait_until_status("SUCCESSFUL")

    def wait_until_fail(self):
        """Wait until c8y reports a fail"""
        self.wait_until_status("FAILED")

    def wait_until_end(self):
        """Wait until c8y reports a fail"""
        self.wait_until_status("FAILED", "SUCCESSFUL")

    def wait_until_status(self, status, status2=False):
        """Wait until c8y reports status or status2."""

        # Heuristic about how long to wait for a operation

        poll_period = 2  # seconds

        if platform.machine() == "x86_64":
            wait_time = int(40 / poll_period)
        else:
            wait_time = int(90 / poll_period)  # 90s on the Rpi

        timeout = 0

        # wait for some time to let c8y process a request until we can poll for it
        time.sleep(poll_period)

        while True:

            if self.operation_id:

                current_status = self.get_status_of_operation()
                if current_status == status or current_status == status2:
                    # Invalidate the old operation
                    self.operation_id = None
                    break

            else:

                current_status = self.get_status_of_last_operation()
                if current_status == status or current_status == status2:
                    # Invalidate the old operation
                    self.operation_id = None
                    break

            time.sleep(poll_period)

            timeout += 1
            if timeout > wait_time:
                raise SystemError(
                    "Timeout while waiting for status %s or %s" % (status, status2)
                )

    def check_is_installed(self, package_name, version=None):
        """Check if a package is installed"""

        url = f"https://{self.tenant_url}/inventory/managedObjects/{self.project.deviceid}"
        req = requests.get(url, headers=self.header, timeout=self.timeout_req)

        req.raise_for_status()

        jresponse = json.loads(req.text)

        ret = False

        package_list = jresponse.get("c8y_SoftwareList")

        for package in package_list:
            if package.get("name") == package_name:
                self.log.info("Package %s is installed", package_name)
                # self.log.info(package)
                if version:
                    if package.get("version") == version:
                        ret = True
                        break

                    raise SystemError("Wrong version is installed")

                ret = True
                break
        return ret

    def getpkgversion(self, pkg):
        """ "Use apt-cache madison to derive a package version from
        the apt cache even when it is not installed.
        Not very bulletproof yet!!!
        """
        output = subprocess.check_output(["/usr/bin/apt-cache", "madison", pkg])

        # Lets assume it is the package in the first line of the output
        return output.split()[2].decode("ascii")  # E.g. "1.16-1+b3"

    def get_pkgid(self, pkg):

        # Database with package IDs
        # TODO use this everywhere
        pkgiddb = {
            # apt
            "asciijump": "5475278",
            "robotfindskitten": "5473003",
            "squirrel3": "5474871",
            "rolldice": "5445239",
            "moon-buggy": "5439204",
            # fake plugin
            "apple": "5495053",
            "banana": "5494888",
            "cherry": "5495382",
            "watermelon": "5494510",
        }

        pkgid = pkgiddb.get(pkg)

        if pkgid:
            return pkgid
        else:
            raise SystemError("Package ID not in database")

    def mysmcleanup(self):
        # Experiment
        time.sleep(5)
