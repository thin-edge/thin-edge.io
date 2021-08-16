from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour

When we install a package that cannot be installed with the apt package manager
Then we receive a failure from the apt plugin
"""

import json
import requests
import time
import sys

sys.path.append("software-management-end-to-end")
from environment_sm_management import SmManagement


class PySysTest(SmManagement):
    def setup(self):
        super().setup()
        self.assertThat("False == value", value=self.check_isinstalled("rolldice"))

    def execute(self):

        self.trigger_action("does_not_exist", "5446165", "::apt", "notanurl", "install")

        self.wait_until_fail()

    def validate(self):

        self.assertThat(
            "False == value", value=self.check_isinstalled("does_not_exist")
        )
