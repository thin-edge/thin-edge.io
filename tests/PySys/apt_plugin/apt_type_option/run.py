import json
import sys

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin


class AptPluginTypeOption(AptPlugin):
    def execute(self):
        self.plugin_cmd("type", "type_output", 0)

    def validate(self):
        output_file = open(self.output + "/type_output.out", "r")
        json_output = json.load(output_file)

        self.assertTrue(len(json_output) == 1)
        self.assertTrue(json_output["type"] == "apt")
