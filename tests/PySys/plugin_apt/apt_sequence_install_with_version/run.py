import sys
import os
import subprocess
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


class AptPluginPrepInstallWithVersionFinalize(AptPlugin):
    def setup(self):
        super().setup()

        self.package = "rolldice"

        # Use apt extension madison-lite to get the version of rolldice easily
        # Output of the call:
        # apt-cache madison rolldice
        # rolldice |  1.16-1+b3 | http://ftp.uni-stuttgart.de/debian bullseye/main amd64 Packages
        # rolldice |     1.16-1 | http://ftp.uni-stuttgart.de/debian bullseye/main Sources
        #
        # On Ubuntu 20.04 it is
        # rolldice | 1.16-1build1 | http://in.archive.ubuntu.com/ubuntu focal/universe amd64 Packages

        output = subprocess.check_output(["/usr/bin/apt-cache", "madison", "rolldice"])

        # Lets assume it is the package in the first line of the output
        self.version = output.split()[2].decode("utf8")  # E.g. "1.16-1+b3"

        self.apt_remove(self.package)
        self.assert_isinstalled(self.package, False)
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):
        self.plugin_cmd("prepare", "outp_prepare", 0)
        self.plugin_cmd(
            "install", "outp_install", 0, argument=self.package, version=self.version
        )
        self.plugin_cmd("finalize", "outp_finalize", 0)

    def validate(self):
        self.assert_isinstalled(self.package, True)
        # This is evaluated as regex therefore we need to excape the plus sign
        # E.g. Expression (1|3) matches either 1 or 3 for debian buster or bullseye
        # On some systems there is an optional plus sign and b instead of build
        self.assertGrep(
            "outp_check_1.out",
            "rolldice\t1.16-1(\+b|build)(1|3)",
            contains=True,
        )

    def cleanup_prepare(self):
        self.apt_remove(self.package)
