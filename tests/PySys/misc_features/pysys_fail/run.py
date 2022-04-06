import sys
import time

from pysys.basetest import BaseTest
from pysys.constants import FAILED

"""
Just fail
"""


class Fail(BaseTest):
    def execute(self):
        self.log.error("I will fail")
        self.addOutcome(FAILED)
