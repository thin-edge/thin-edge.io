import pysys
from pysys.basetest import BaseTest

"""
Validate apt plugin output format
"""


class AptPluginTest(BaseTest):
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
            arguments=['--show', '--showformat=${Version}\\n', 'grep'],
            stdouterr="dpkg_query",
        )

    def validate(self):
        grep_version = open(self.output + '/dpkg_query.out', 'r').read().strip()

        # Assuming grep is installed
        self.assertGrep ("apt_plugin.out", '{"name":"grep","version":"'+ grep_version + '"}', contains=True)

        # systemd is installed but should not be listed
        self.assertGrep ("apt_plugin.out", '"name":"systemd"', contains=False)
