import sys
import os
import time

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin remove a specific versioned module use case

When we remove a package module with version as below
When that version is not installed
Remove will fail
Grep for the specific error message

sudo /etc/tedge/sm-plugins/apt remove rolldice  --module-version 1.111111

"""


class AptPluginRemoveWithVersionFails(AptPlugin):
    def setup(self):
        super().setup()
        self.version = "1.111111"
        self.package = "rolldice"
        self.plugin_cmd("install", "outp_install", 0, argument=self.package)
        self.addCleanupFunction(self.cleanup_remove_rolldice_module)

    def execute(self):
        self.plugin_cmd(
            "remove", "outp_remove", 2, argument=self.package, version=self.version
        )

    def validate(self):
        self.assertGrep("outp_remove.err", "1.111111")
        self.assert_isinstalled(self.package, True)

    def cleanup_remove_rolldice_module(self):
        self.apt_remove("rolldice")
