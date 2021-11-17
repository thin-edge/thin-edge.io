import sys
import os
import time

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin remove use case

When we install a package
When we prepare
When we remove a package
When we finalize
Then the package is not installed
"""


class AptPluginInstallPrepareRemoveFinalize(AptPlugin):
    def setup(self):
        super().setup()

        self.package = "rolldice"
        self.apt_install(self.package)
        self.assert_isinstalled(self.package, True)
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):
        self.plugin_cmd("prepare", "outp_prepare", 0)
        self.plugin_cmd("remove", "outp_remove", 0, self.package)
        self.plugin_cmd("finalize", "outp_finalize", 0)

    def validate(self):
        self.assert_isinstalled(self.package, False)

    def cleanup_prepare(self):
        # Just to make sure it is away
        self.apt_remove(self.package)
