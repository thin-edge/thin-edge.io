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

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat(
            "False == value", value=self.check_is_installed("robotfindskitten")
        )

    def execute(self):

        # The ID is currently hardcoded to the IDs for tenant thin-edge-io
        # TODO Improve repository ID management
        action = [
            {
                "action": "install",
                "id": "5475278",
                "name": "asciijump",
                "url": " ",
                "version": "::apt",
            },
            {
                "action": "install",
                "id": "5473003",
                "name": "robotfindskitten",
                "url": " ",
                "version": "::apt",
            },
            {
                "action": "install",
                "id": "5474871",
                "name": "squirrel3",
                "url": " ",
                "version": "::apt",
            },
        ]

        self.trigger_action_json(action)

        self.wait_until_succcess()

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

        self.assertThat("True == value", value=self.check_is_installed("asciijump"))
        self.assertThat("True == value", value=self.check_is_installed("squirrel3"))
        self.assertThat(
            "True == value", value=self.check_is_installed("robotfindskitten")
        )

        # The ID is currently hardcoded to the IDs for tenant thin-edge-io
        # TODO Improve repository ID management
        action = [
            {
                "action": "delete",
                "id": "5475278",
                "name": "asciijump",
                "url": " ",
                "version": "::apt",
            },
            {
                "action": "delete",
                "id": "5473003",
                "name": "robotfindskitten",
                "url": " ",
                "version": "::apt",
            },
            {
                "action": "delete",
                "id": "5474871",
                "name": "squirrel3",
                "url": " ",
                "version": "::apt",
            },
        ]

        self.trigger_action_json(action)

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_is_installed("rolldice"))
        self.assertThat("False == value", value=self.check_is_installed("asciijump"))
        self.assertThat("False == value", value=self.check_is_installed("squirrel3"))
        self.assertThat(
            "False == value", value=self.check_is_installed("robotfindskitten")
        )
