from environment_apt_plugin import AptPlugin
import sys
import os
import subprocess
import time

sys.path.append("apt_plugin")

"""
Validate apt plugin update-list command use case

When we install/remove multiple packages at once with update-list command
Then these packages are installed/removed together

Issue:
whenever there is a parameter --version the output will be "apt-install 0.1.0"

sudo /etc/tedge/sm-plugins/apt install rolldice 1.16-1+b3 --version
apt-install 0.1.0
"""


class AptPluginUpdateList(AptPlugin):

    update_list = 'update-list'

    def setup(self):
        super().setup()

        self.package1 = "rolldice"
        self.package2 = "asciijump"
        self.package3 = "moon-buggy"

        self.apt_remove(self.package1)
        self.apt_install(self.package2)
        self.apt_remove(self.package3)

        self.assert_isinstalled(self.package1, False)
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):
        os.system('{} {} {} < {}'.format(self.sudo,
                  self.apt_plugin, self.update_list, self.input + "/update_list_input"))

    def validate(self):
        self.assert_isinstalled(self.package1, True)
        self.assert_isinstalled(self.package2, False)
        self.assert_isinstalled(self.package3, True)

    def cleanup_prepare(self):
        self.apt_remove(self.package1)
        self.apt_remove(self.package2)
        self.apt_remove(self.package3)
