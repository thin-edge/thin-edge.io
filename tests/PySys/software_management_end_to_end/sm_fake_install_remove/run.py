from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the dummy plugin for installation and removal of packages

When we install a package
Then it is installed
When we deinstall it again
Then it is not installed

This test is currently skipped as it needs a specialized setup with the
dummy-plugin set up to install fruits.

To run it do this:
    pysys.py run -v DEBUG 'sm-fake*' -Xfakeplugin=fakeplugin -XmyPlatform=container

"""

import json
import requests
import time
import sys

from environment_sm_management import SoftwareManagement


class PySysTest(SoftwareManagement):
    def setup(self):

        if self.fakeplugin != "fakeplugin":
            self.skipTest(
                "Testing the fake plugin is not enabled."
                + "Use parameter -Xfakeplugin=fakeplugin to enable it"
            )

        super().setup()

        # This is always false
        self.assertThat("False == value", value=self.check_is_installed("tomatoooo"))

    def execute(self):

        self.trigger_action(
            "watermelon", self.get_pkgid("watermelon"), "::fruits", "", "install"
        )

        self.wait_until_succcess()

        # vanilla is always there
        self.assertThat("True == value", value=self.check_is_installed("vanilla"))

        self.trigger_action(
            "watermelon", self.get_pkgid("watermelon"), "::fruits", "", "delete"
        )

        self.wait_until_succcess()

    def validate(self):
        # This is always false
        self.assertThat("False == value", value=self.check_is_installed("tomatoooo"))
