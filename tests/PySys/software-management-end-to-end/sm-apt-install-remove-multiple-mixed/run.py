from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for multiple packages

When we install a bunch of packages
Then they are installed
When we deinstall them again
Then they are not installed
"""

import json
import requests
import time
import sys

sys.path.append("software-management-end-to-end")
from environment_sm_management import SoftwareManagement


class PySysTest(SoftwareManagement):
    def setup(self):
        super().setup()

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))

    def execute(self):

        pkgid = {
            # apt
            "asciijump": "5475278",
            "robotfindskitten": "5473003",
            "squirrel3": "5474871",
            "rolldice": "5445239",
        }

        act = "install"
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
                "version": "3.1-8::apt", # verson and manager
            },
            {
                "action": act,
                "id": pkgid["rolldice"],
                "name": "rolldice",
                "url": " ",
                "version": "1.16-1+b3", # only version
            },
        ]

        self.trigger_action_json(action)

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("asciijump"))

        act = "delete"
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
                "version": "3.1-8::apt", # verson and manager
            },
            {
                "action": act,
                "id": pkgid["rolldice"],
                "name": "rolldice",
                "url": " ",
                "version": "1.16-1+b3", # only version
            },
        ]

        self.trigger_action_json(action)

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
