from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for multiple packages

When we install a bunch of packages with versions, without and even one twice
Then they are installed
When we deinstall them again
Then they are not installed
"""

import time
import subprocess
import sys

sys.path.append("software-management-end-to-end")
from environment_sm_management import SoftwareManagement

def getversion(pkg):
        output = subprocess.check_output(["/usr/bin/apt-cache", "madison", pkg])

        # Lets assume it is the package in the first line of the output
        return output.split()[2].decode('ascii')  # E.g. "1.16-1+b3"

def getaction(act):

        pkgid = {
            # apt
            "asciijump": "5475278",
            "robotfindskitten": "5473003",
            "squirrel3": "5474871",
            "rolldice": "5445239",
            "moon-buggy": "5439204",
        }

        action = [
            {
                "action": act,
                "id": pkgid["asciijump"],
                "name": "asciijump",
                "url": " ",
                "version": "::apt", # apt manager
            },
            {
                "action": act,
                "id": pkgid["robotfindskitten"],
                "name": "robotfindskitten",
                "url": " ",
                "version": " ", # default manager
            },
            {
                "action": act,
                "id": pkgid["squirrel3"],
                "name": "squirrel3",
                "url": " ",
                "version": getversion("squirrel3")+"::apt", # verson and manager
            },
            {
                "action": act,
                "id": pkgid["rolldice"],
                "name": "rolldice",
                "url": " ",
                "version": getversion("rolldice"), # only version
            },
            {
                "action": act,
                "id": pkgid["moon-buggy"],
                "name": "moon-buggy",
                "url": " ",
                "version": getversion("moon-buggy"), # nothing special
            },
            {
                "action": act,
                "id": pkgid["asciijump"],
                "name": "asciijump",
                "url": " ",
                "version": "::apt", # again same as above
            },        ]
        return action


class PySysTest(SoftwareManagement):
    def setup(self):
        super().setup()

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat("False == value", value=self.check_is_installed("robotfindskitten"))
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat("False == value", value=self.check_is_installed("moon-buggy"))

    def execute(self):

        action = getaction("install")

        self.trigger_action_json(action)

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("asciijump"))
        self.assertThat("True == value", value=self.check_is_installed("robotfindskitten"))
        self.assertThat("True == value", value=self.check_is_installed("rolldice"))
        self.assertThat("True == value", value=self.check_is_installed("squirrel3"))
        self.assertThat("True == value", value=self.check_is_installed("moon-buggy"))

        action = getaction("delete")

        self.trigger_action_json(action)

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat("False == value", value=self.check_is_installed("robotfindskitten"))
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat("False == value", value=self.check_is_installed("moon-buggy"))
