import sys

sys.path.append("apt-plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin behaviour when we use nonsense parameters

"""


class AptPluginWrongOption(AptPlugin):
    def setup(self):
        super().setup()

    def execute(self):
        self.plugin_cmd("nonsense", "outp", 1)
