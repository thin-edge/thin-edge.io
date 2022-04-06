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

from environment_sm_management import SoftwareManagement


class PySysTest(SoftwareManagement):
    def get_packages_with_action(self, act):
        "create an action that we can use later"

        action = [
            {
                "action": act,
                "id": self.get_pkgid("asciijump"),
                "name": "asciijump",
                "url": " ",
                "version": "::apt",  # apt manager
            },
            {
                "action": act,
                "id": self.get_pkgid("robotfindskitten"),
                "name": "robotfindskitten",
                "url": " ",
                "version": "::apt",  # apt manager
            },
            {
                "action": act,
                "id": self.get_pkgid("squirrel3"),
                "name": "squirrel3",
                "url": " ",
                "version": "::apt",  # apt manager
            },
        ]
        return action

    def setup(self):
        super().setup()

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat(
            "False == value", value=self.check_is_installed("robotfindskitten")
        )

    def execute(self):

        action = self.get_packages_with_action("install")
        self.trigger_action_json(action)
        self.wait_until_succcess()

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

        self.assertThat("True == value", value=self.check_is_installed("asciijump"))
        self.assertThat("True == value", value=self.check_is_installed("squirrel3"))
        self.assertThat(
            "True == value", value=self.check_is_installed("robotfindskitten")
        )

        action = self.get_packages_with_action("delete")
        self.trigger_action_json(action)
        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat(
            "False == value", value=self.check_is_installed("robotfindskitten")
        )
