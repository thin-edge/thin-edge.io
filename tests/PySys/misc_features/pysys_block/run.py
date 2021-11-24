import sys
import time

from pysys.basetest import BaseTest
from pysys.constants import BLOCKED

"""
Just fail
"""

class Fail(BaseTest):

    def execute(self):
        self.log.error("I will block")
        self.addOutcome(BLOCKED)

