from pysys.basetest import BaseTest

import time

"""
Validate ...

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

        self.trigger_action("rolldice", "5445239", "::apt", "notanurl", "install")

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_isinstalled("rolldice"))

        self.trigger_action("rolldice", "5445239", "::apt", "notanurl", "delete")

        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value", value=self.check_isinstalled("rolldice"))
