from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for a failing installation

When we install a package that cannot be installed with the apt package manager
Then we receive a failure from the apt plugin
"""

import time
import sys

from environment_sm_management import SoftwareManagement


class PySysTest(SoftwareManagement):
    def setup(self):
        super().setup()

    def execute(self):

        self.trigger_action("does_not_exist", "5446165", "::apt", "", "install")

        self.wait_until_fail()

    def validate(self):

        self.assertThat(
            "False == value", value=self.check_is_installed("does_not_exist")
        )
