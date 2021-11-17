import sys

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin install fails

When we install a non existing package
Then we expect an error code from the plugin
"""


class AptPluginInstallFail(AptPlugin):
    def setup(self):
        super().setup()
        self.assert_isinstalled("notapackage", False)

    def execute(self):
        self.plugin_cmd("install", "outp_install", 2, "notapackage")

    def validate(self):
        self.assert_isinstalled("notapackage", False)
