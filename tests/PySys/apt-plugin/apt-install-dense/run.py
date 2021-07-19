import pysys
from pysys.basetest import BaseTest

"""
Validate apt plugin install

Using `rolldice` as a guinea pig: [small and without impacts](https://askubuntu.com/questions/422362/very-small-package-for-apt-get-experimentation)
"""

class AptPlugin(BaseTest):

    def setup(self):
        self.apt_plugin = "/etc/tedge/sm-plugins/apt"
        self.apt_get = "/usr/bin/apt-get"
        self.sudo = "/usr/bin/sudo"


    def plugin_call( self, command, outputfile, exit_code, argument):
        process = self.startProcess(
            command=self.sudo,
            arguments=[self.apt_plugin, command],
            stdouterr=outputfile,
            expectedExitStatus=f"=={exit_code}",
        )

class AptPluginInstallTest(AptPlugin):
    def setup(self):
        super().setup()
        self.remove_rolldice_module()
        self.addCleanupFunction(self.remove_rolldice_module)

    def execute(self):

        self.plugin_call('list', 'before', 0)


        install = self.startProcess(
            command=self.sudo,
            arguments=[self.apt_plugin, "install", "rolldice"],
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
        self.assertGrep ("before.out", 'rolldice', contains=False)
        self.assertGrep ("after.out", 'rolldice', contains=True)

    def remove_rolldice_module(self):
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, 'remove', '-y', 'rolldice'],
            abortOnError=False,
        )
