from environment_apt_plugin import AptPlugin
import sys

sys.path.append("apt_plugin")


class AptPluginTypeOption(AptPlugin):
    def execute(self):
        self.plugin_cmd("type", "type_output", 1)

    def validate(self):
        self.assertGrep("type_output.out", "debian")
