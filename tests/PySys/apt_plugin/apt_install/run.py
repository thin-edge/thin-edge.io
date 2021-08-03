import sys

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin install

Using `rolldice` as a guinea pig: [small and without impacts](https://askubuntu.com/questions/422362/very-small-package-for-apt-get-experimentation)

When we list all packages
When we install a package
Then we find the package in the list of installed packages
"""


class AptPluginInstallDense(AptPlugin):
    def setup(self):
        super().setup()
        self.apt_remove("rolldice")
        self.assert_isinstalled("rolldice", False)
        self.addCleanupFunction(self.cleanup_remove_rolldice_module)

    def execute(self):
        self.plugin_cmd("install", "outp_install", 0, "rolldice")

    def validate(self):
        self.assert_isinstalled("rolldice", True)

    def cleanup_remove_rolldice_module(self):
        self.apt_remove("rolldice")
