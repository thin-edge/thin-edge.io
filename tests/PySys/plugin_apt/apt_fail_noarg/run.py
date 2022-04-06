import sys

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin calls fail without/wrong paramters

When we install/remove without a package
Then we expect an error code from the plugin
"""


class AptPluginFailNoArg(AptPlugin):
    def setup(self):
        super().setup()

    def execute(self):
        self.plugin_cmd("install", "outp_install", 1)
        self.plugin_cmd("remove", "outp_install", 1)
        self.plugin_cmd("prepare", "outp_install", 1, "nonsense")
        self.plugin_cmd("list", "outp_install", 1, "nonsense")
        self.plugin_cmd("finalize", "outp_install", 1, "nonsense")

        self.plugin_cmd(
            "install", "outp_install", 1, "rolldice", "nonsense", "nonsense"
        )
        self.plugin_cmd("remove", "outp_install", 1, "rolldice", "nonsense", "nonsense")

        self.plugin_cmd("prepare", "outp_install", 1, "nonsense", "nonsense")
        self.plugin_cmd("list", "outp_install", 1, "nonsense", "nonsense")
        self.plugin_cmd("finalize", "outp_install", 1, "nonsense", "nonsense")

        # self.skipTest('MyFeature is not supported on Windows')
        # logging.info('')
