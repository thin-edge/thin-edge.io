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
            arguments=['--show', '--showformat=${Version}\\n', 'openssl'],
            stdouterr="dpkg_query",
        )

    def validate(self):
        openssl_version = open(self.output + '/dpkg_query.out', 'r').read().strip()

        self.assertGrep ("apt_plugin.out", '{"name":"openssl","version":"'+ openssl_version + '"}', contains=True)
