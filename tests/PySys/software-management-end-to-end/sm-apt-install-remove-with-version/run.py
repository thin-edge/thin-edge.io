from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for packages with version

When we install a package
Then it is installed
When we deinstall it again with the wrong version
Then it is still installed
When we deinstall it again with the right version
Then it is not installed

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
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

    def execute(self):

        self.trigger_action(
            "rolldice", "5445239", "1.16-1+b1::apt", "notanurl", "install"
        )

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("rolldice"))

        self.trigger_action("rolldice", "5445239", "88::apt", "notanurl", "delete")

        self.wait_until_fail()

        self.assertThat("True == value", value=self.check_is_installed("rolldice"))

        self.trigger_action(
            "rolldice", "5445239", "1.16-1+b1::apt", "notanurl", "delete"
        )

        self.wait_until_succcess()

    def validate(self):

        self.assertThat(
            "False == value",
            value=self.check_is_installed("rolldice", "1.16-1+b1::apt"),
        )
