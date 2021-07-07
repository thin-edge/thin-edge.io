import pysys
from pysys.basetest import BaseTest

"""
Validate apt plugin remove

Using `rolldice` as a guinea pig: [small and without impacts](https://askubuntu.com/questions/422362/very-small-package-for-apt-get-experimentation)
"""

class AptPluginRemoveTest(BaseTest):
    def setup(self):
        self.apt_plugin = "/etc/tedge/sm-plugins/apt"
        self.apt_get = "/usr/bin/apt-get"
        self.sudo = "/usr/bin/sudo"

        self.install_rolldice_module()
        self.addCleanupFunction(self.remove_rolldice_module)

    def execute(self):
        before = self.startProcess(
            command=self.sudo,
            arguments=[self.apt_plugin, "list"],
            stdouterr="before",
            expectedExitStatus="==0",
        )

        install = self.startProcess(
            command=self.sudo,
            arguments=[self.apt_plugin, "remove", "rolldice"],
            stdouterr="install",
            expectedExitStatus="==0",
        )

        after = self.startProcess(
            command=self.sudo,
            arguments=[self.apt_plugin, "list"],
            stdouterr="after",
            expectedExitStatus="==0",
        )

    def validate(self):
        self.assertGrep ("before.out", 'rolldice', contains=True)
        self.assertGrep ("after.out", 'rolldice', contains=False)

    def install_rolldice_module(self):
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, 'install', '-y', 'rolldice'],
        )

    def remove_rolldice_module(self):
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, 'remove', '-y', 'rolldice'],
        )
