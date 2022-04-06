from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for multiple packages with mixed versions

When we install a bunch of packages with versions, without and even one twice
Then they are installed
When we deinstall them again
Then they are not installed
"""

import time
import subprocess
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
                "version": " ",  # default manager
            },
            {
                "action": act,
                "id": self.get_pkgid("squirrel3"),
                "name": "squirrel3",
                "url": " ",
                "version": self.get_pkg_version("squirrel3")
                + "::apt",  # version and manager
            },
            {
                "action": act,
                "id": self.get_pkgid("rolldice"),
                "name": "rolldice",
                "url": " ",
                "version": self.get_pkg_version("rolldice"),  # only version
            },
            {
                "action": act,
                "id": self.get_pkgid("moon-buggy"),
                "name": "moon-buggy",
                "url": " ",
                "version": self.get_pkg_version("moon-buggy"),  # nothing special
            },
            {
                "action": act,
                "id": self.get_pkgid("asciijump"),
                "name": "asciijump",
                "url": " ",
                "version": "::apt",  # again same as above
            },
        ]
        return action

    def setup(self):
        super().setup()

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat(
            "False == value", value=self.check_is_installed("robotfindskitten")
        )
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat("False == value", value=self.check_is_installed("moon-buggy"))

    def execute(self):

        action = self.get_packages_with_action("install")

        self.trigger_action_json(action)

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("asciijump"))
        self.assertThat(
            "True == value", value=self.check_is_installed("robotfindskitten")
        )
        self.assertThat("True == value", value=self.check_is_installed("rolldice"))
        self.assertThat("True == value", value=self.check_is_installed("squirrel3"))
        self.assertThat("True == value", value=self.check_is_installed("moon-buggy"))

        action = self.get_packages_with_action("delete")

        self.trigger_action_json(action)

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat(
            "False == value", value=self.check_is_installed("robotfindskitten")
        )
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat("False == value", value=self.check_is_installed("moon-buggy"))
