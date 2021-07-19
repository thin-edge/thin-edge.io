import sys

sys.path.append("apt-plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin install

Using `rolldice` as a guinea pig: [small and without impacts](https://askubuntu.com/questions/422362/very-small-package-for-apt-get-experimentation)

When we list all packages
When we install a package
Then we find the package in the list of installed packages
"""


class AptPluginInstallTest(AptPlugin):
    def setup(self):
        super().setup()
        self.remove_rolldice_module()
        self.addCleanupFunction(self.remove_rolldice_module)

    def execute(self):
        self.plugin_cmd("list", "outp_before", 0)
        self.plugin_cmd("install", "outp_install", 0, "rolldice")
        self.plugin_cmd("list", "outp_after", 0)

    def validate(self):
        self.assertGrep("outp_before.out", "rolldice", contains=False)
        self.assertGrep("outp_after.out", "rolldice", contains=True)

    def remove_rolldice_module(self):
        self.apt_remove("rolldice")
