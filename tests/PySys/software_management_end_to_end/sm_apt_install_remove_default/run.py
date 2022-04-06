from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for installation and removal of packages

For the default plugin with a space as version

When we install a package
Then it is installed
When we deinstall it again
Then it is not installed
"""

import json
import requests
import time
import sys

from environment_sm_management import SoftwareManagement


class PySysTest(SoftwareManagement):
    def setup(self):
        super().setup()
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

    def execute(self):

        self.trigger_action("rolldice", self.get_pkgid("rolldice"), " ", "", "install")

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("rolldice"))

        self.trigger_action("rolldice", self.get_pkgid("rolldice"), " ", "", "delete")

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
