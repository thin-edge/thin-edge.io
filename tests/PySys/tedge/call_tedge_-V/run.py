import os

import toml
import pysys
from pysys.basetest import BaseTest

import time

"""
Validate command line option -V

Note: this is a static check and needs to be updated when a
version switch occurs.

Given a running system
When we call tedge -V
Then we find the string tedge 0.1.0 in the output
"""


class PySysTest(BaseTest):
    def setup(self):

        # retrieve the expected toml version from the Cargo.toml
        tomlname = os.path.join(self.project.tebasedir, "crates/core/tedge/Cargo.toml")
        with open(tomlname, "r") as tomlfile:
            tedgetoml = toml.load(tomlfile)
            self.tedgeversion = tedgetoml["package"]["version"]

    def execute(self):
        tedge = "/usr/bin/tedge"

        proc = self.startProcess(
            command=tedge,
            arguments=["-V"],
            stdouterr="tedge",
            expectedExitStatus="==0",
        )

    def validate(self):
        self.assertGrep("tedge.out", f"tedge {self.tedgeversion}", contains=True)
