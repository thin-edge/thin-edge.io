from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for installation and removal of packages

For the apt plugin with ::apt

When we install a package
Then it is installed
When we deinstall it again
Then it is not installed
"""

import json
import requests
import time
import subprocess
import sys

from environment_sm_management import SoftwareManagement


class PySysTest(SoftwareManagement):
    def setup(self):
        super().setup()
        if self.check_is_installed("rolldice"):
            self.remove_package_apt("rolldice")
            self.assertThat("0 == value", value=proc.return_value)

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

    def execute(self):

        self.trigger_action(
            "rolldice", self.get_pkgid("rolldice"), "::apt", "", "install"
        )

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("rolldice"))

        self.trigger_action(
            "rolldice", self.get_pkgid("rolldice"), "::apt", "", "delete"
        )

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
