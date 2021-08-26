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
"""


class AptPluginPrepareInstallFinalize(AptPlugin):
    def setup(self):
        super().setup()

        self.package = "rolldice"
        self.apt_remove(self.package)
        self.assert_isinstalled(self.package, False)
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):
        self.plugin_cmd("prepare", "outp_prepare", 0)
        self.plugin_cmd("install", "outp_install", 0, self.package)
        self.plugin_cmd("finalize", "outp_finalize", 0)

    def validate(self):
        self.assert_isinstalled(self.package, True)

    def cleanup_prepare(self):
        self.apt_remove(self.package)
