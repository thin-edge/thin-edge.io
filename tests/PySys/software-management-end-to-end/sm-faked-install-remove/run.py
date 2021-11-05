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
from environment_sm_management import SoftwareManagement

class PySysTest(SoftwareManagement):
    def setup(self):

        if self.fakeplugin != "fakeplugin":
            self.skipTest("Testing the fake plugin is not supported on this platform"+\
                    "Use parameter -Xfakeplugin=fakeplugin to enable it")

        super().setup()
        self.assertThat("False == value", value=self.check_is_installed("tomatoooo"))

    def execute(self):

        self.trigger_action("watermelon", "5494510", "::fruits", "notanurl", "install")

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("vanilla"))

        self.trigger_action("watermelon", "5494510", "::fruits", "notanurl", "delete")

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("tomatoooo"))
