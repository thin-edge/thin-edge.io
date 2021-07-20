import pysys
from pysys.basetest import BaseTest
import json

"""
Validate apt plugin output format
"""


class AptPluginListTest(BaseTest):
    def execute(self):
        apt_plugin = "/etc/tedge/sm-plugins/apt"

        proc = self.startProcess(
            command=apt_plugin,
            arguments=["list"],
            stdouterr="apt_plugin",
            expectedExitStatus="==0",
        )

        self.startProcess(
            command="/usr/bin/dpkg-query",
            arguments=["--show", "--showformat=${Version}\\n", "grep"],
            stdouterr="dpkg_query",
        )

    def validate(self):
        self.validate_json()

        # Assuming grep is installed
        grep_version = open(self.output + "/dpkg_query.out", "r").read().strip()
        self.assertGrep(
            "apt_plugin.out",
            '{"name":"grep","version":"' + grep_version + '"}',
            contains=True,
        )

    def validate_json(self):
        f = open(self.output + "/apt_plugin.out", "r")
        lines = f.readlines()
        for line in lines:
            self.js_msg = json.loads(line)
            if not self.js_msg["name"]:
                reason = "missing module name in: " + str(line)
                self.abort(False, reason)
            if not self.js_msg["version"]:
                reason = "missing module version in: " + str(line)
                self.abort(False, reason)
