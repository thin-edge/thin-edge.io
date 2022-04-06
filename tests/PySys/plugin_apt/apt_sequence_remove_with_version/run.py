import sys
import os
import time
import subprocess

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""

Validate that package removal with version works well
When we make sure a package is installed
When we remove that package with the version that is currently installed
Then the package is not installed anymore

sudo /etc/tedge/sm-plugins/apt install rolldice 1.16-1+b3 --version
apt-install 0.1.0


"""


class AptPluginRemoveWithVersion(AptPlugin):
    def setup(self):
        super().setup()

        output = subprocess.check_output(["/usr/bin/apt-cache", "madison", "rolldice"])
        # Lets assume it is the package in the first line of the output
        self.version = output.split()[2].decode("utf8")  # E.g. "1.16-1+b3"
        # self.version = "1.16-1build1"
        self.package = "rolldice"
        self.apt_remove(self.package)
        self.plugin_cmd("install", "outp_install", 0, "rolldice")
        self.assert_isinstalled(self.package, True)
        self.addCleanupFunction(self.cleanup_remove_rolldice_module)

    def execute(self):
        self.plugin_cmd(
            "remove", "outp_install", 0, argument=self.package, version=self.version
        )

    def validate(self):
        self.assert_isinstalled(self.package, False)

    def cleanup_remove_rolldice_module(self):
        self.apt_remove("rolldice")
