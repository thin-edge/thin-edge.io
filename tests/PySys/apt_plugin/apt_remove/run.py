import sys

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin remove

When we install a package
When we remove the package
Then we dont find it in the list of installed packages
"""


class AptPluginRemoveDense(AptPlugin):
    def setup(self):
        super().setup()
        self.plugin_cmd("install", "outp_install", 0, "rolldice")
        self.assert_isinstalled("rolldice", True)

    def execute(self):
        self.plugin_cmd("remove", "outp_remove", 0, "rolldice")

    def validate(self):
        self.assert_isinstalled("rolldice", False)
