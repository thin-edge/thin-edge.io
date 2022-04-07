import re
import pysys
from pysys.basetest import BaseTest

"""
Validate apt plugin output format
"""


class AptPluginListFormat(BaseTest):
    def execute(self):
        apt_plugin = "/etc/tedge/sm-plugins/apt"

        proc = self.startProcess(
            command=apt_plugin,
            arguments=["list"],
            stdouterr="apt_plugin",
            expectedExitStatus="==0",
        )

        # dpkg-querry outputs tab separated 'module\tversion` pair
        # for example `dash    0.5.10.2-6`
        self.startProcess(
            command="/usr/bin/dpkg-query",
            arguments=["--showformat=${Package}\\t${Version}\\n", "--show", "dash"],
            stdouterr="dpkg_query",
        )

    def validate(self):
        # Assuming `dash` is installed
        dash_package_info = open(self.output + "/dpkg_query.out", "r").read().strip()

        # On debian bullyeye this contains plus signs, that we need to
        # escape until we process the regex.
        # Example: dash\t0.5.11+git20200708+dd9ef66-5
        dash_package_info = dash_package_info.replace("+", "\+")

        self.assertGrep(
            "apt_plugin.out",
            dash_package_info,
            contains=True,
        )
