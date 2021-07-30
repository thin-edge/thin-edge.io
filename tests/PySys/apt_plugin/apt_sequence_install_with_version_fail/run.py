import sys
import os
import time

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin install use case

When we prepare
When we install a package
When we finalize
Then the package is installed

Issue:
whenever there is a parameter --version the output will be "apt-install 0.1.0"

sudo /etc/tedge/sm-plugins/apt install rolldice 1.16-1+b3 --version
apt-install 0.1.0


"""


class AptPluginInstallWithVersionFails(AptPlugin):
    def setup(self):
        super().setup()

        self.package = "rolldice"
        self.version = "1.16-1+b0"  # Version is too old
        self.apt_remove(self.package)
        self.assert_isinstalled(self.package, False)

    def execute(self):
        self.plugin_cmd(
            "install", "outp_install", 2, argument=self.package, version=self.version
        )

    def validate(self):
        self.assert_isinstalled(self.package, False)
