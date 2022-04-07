import time
from pysys.basetest import BaseTest
import subprocess
from threading import Timer

"""
Validate tedge-agent user has a limited sudo right

Given tedge_apt_plugin and tedge_agent are installed
When we run plugin located in plugin directory as tedge-agent
Then a plugin is executed
When we run plugin located out of plugin directory as tedge-agent
Then a plugin is not executed
"""


class TedgeAgentUserSudoAccess(BaseTest):
    def setup(self):
        self.sudo = "/usr/bin/sudo"

        self.log.info("Copy apt plugin 'deb'")
        self.startProcess(
            command=self.sudo,
            arguments=["cp", "/etc/tedge/sm-plugins/apt", "/etc/tedge/sm-plugins/deb"],
            stdouterr="copy_apt_plugin",
        )
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        proc1 = self.startProcess(
            command=self.sudo,
            arguments=["-u", "tedge-agent", self.sudo, "/etc/tedge/sm-plugins/apt"],
            stdouterr="apt",
            expectedExitStatus="==1",
        )
        self.assertThat("value" + proc1.expectedExitStatus, value=proc1.exitStatus)

        proc2 = self.startProcess(
            command=self.sudo,
            arguments=["-u", "tedge-agent", self.sudo, "/etc/tedge/sm-plugins/deb"],
            stdouterr="deb",
            expectedExitStatus="==1",
        )
        self.assertThat("value" + proc2.expectedExitStatus, value=proc2.exitStatus)

        # To Do
        # vulnerability check
        # sudo -u tedge-agent sudo /etc/tedge/sm-plugins/../../../bin/ls
        # Must be asked a password of tedge-agent

    def mycleanup(self):
        self.log.info("Remove the copied apt 'deb' plugin")
        self.startProcess(
            command=self.sudo,
            arguments=["rm", "/etc/tedge/sm-plugins/deb"],
            stdouterr="remove_copied_apt_plugin",
        )
