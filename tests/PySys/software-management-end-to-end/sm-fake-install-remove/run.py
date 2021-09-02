from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for installation and remove

When we install a package
Then it is installed
When we deinstall it again
Then it is not installed

"""

import json
import requests
import time
import sys

sys.path.append("software-management-end-to-end")
from environment_sm_management import SmManagement

# apple 5495053
# banana 5494888
# cherry 5495382
# "watermelon", "5494510"


class PySysTest(SmManagement):
    def setup(self):

        if self.myPlatform != "faked-plugin":
            self.skipTest("Testing the apt plugin is not supported on this platform")

        super().setup()
        self.assertThat("False == value", value=self.check_isinstalled("tomatoooo"))

    def execute(self):

        self.trigger_action("watermelon", "5494510", "::fruits", "notanurl", "install")

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_isinstalled("vanilla"))

        self.trigger_action("watermelon", "5494510", "::fruits", "notanurl", "delete")

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_isinstalled("tomatoooo"))
