from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the dummy-plugin for multiple packages

When we install a bunch of packages
Then they are installed
When we deinstall them again
Then they are not installed

This test is currently skipped as it needs a specialized setup with the
dummy-plugin set up to install fruits.

To run it do this:
    pysys.py run -v DEBUG 'sm-fake*' -Xfakeplugin=fakeplugin -XmyPlatform=smcontainer

"""

import json
import requests
import time
import sys

from environment_sm_management import SoftwareManagement


class PySysTest(SoftwareManagement):

    def get_packages_with_action(self, act):
        "create an action that we can use later"

        mgt = "::fruits"
        act = "install"
        action = [
            {
                "action": act,
                "id": self.get_pkgid("apple"),
                "name": "apple",
                "url": " ",
                "version": mgt,
            },
            {
                "action": act,
                "id": self.get_pkgid("banana"),
                "name": "banana",
                "url": " ",
                "version": mgt,
            },
            {
                "action": act,
                "id": self.get_pkgid("cherry"),
                "name": "cherry",
                "url": " ",
                "version": mgt,
            },
        ]
        return action

    def setup(self):

        if self.fakeplugin != "fakeplugin":
            self.skipTest("Testing the fake plugin is not enabled."+\
                    "Use parameter -Xfakeplugin=fakeplugin to enable it")

        super().setup()

        # note: in the plugin response they are always there
        self.assertThat("True == value", value=self.check_is_installed("apple"))
        self.assertThat("True == value", value=self.check_is_installed("banana"))
        self.assertThat("True == value", value=self.check_is_installed("cherry"))

    def execute(self):

        action = self.get_packages_with_action("install")
        self.trigger_action_json(action)
        self.wait_until_succcess()

        # note: in the plugin response they are always there
        self.assertThat("True == value", value=self.check_is_installed("apple"))
        self.assertThat("True == value", value=self.check_is_installed("banana"))
        self.assertThat("True == value", value=self.check_is_installed("cherry"))

        action = self.get_packages_with_action("delete")
        self.trigger_action_json(action)
        self.wait_until_succcess()

    def validate(self):

        # note: in the plugin response they are always there
        self.assertThat("True == value", value=self.check_is_installed("apple"))
        self.assertThat("True == value", value=self.check_is_installed("banana"))
        self.assertThat("True == value", value=self.check_is_installed("cherry"))
