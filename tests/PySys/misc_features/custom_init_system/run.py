from pysys.basetest import BaseTest
import os

"""
Validate tedge connect/disconnect use a given init system config

Given a configured system
Given a dummy init system executable, that prints out to /etc/tedge/dummy_init.out
Given an init system config to /etc/tedge
When we run tedge connect c8y
When we run tedge disconnect c8y
Then we find no error is recorded on the output of tedge connect/disconnect, also the dummy init system executable.
Cleanup the system config file, the dummy init system executable, and the output of the dummy init system executable.
"""


class CustomInitSystem(BaseTest):
    def setup(self):
        # self.tedge = "/usr/bin/tedge"
        self.tedge = "/home/rina/thinedge/thin-edge-fork/target/debug/tedge"
        self.sudo = "/usr/bin/sudo"
        self.system_conf = "/etc/tedge/system.toml"
        self.dummy_init = "/etc/tedge/dummy_init.sh"
        self.dummy_init_output = "/etc/tedge/dummy_init.out"

        test_dir = os.getcwd() + "/misc_features/custom_init_system"

        # Copy system.toml file to /etc/tedge
        copy_system_config_file = self.startProcess(
            command=self.sudo,
            arguments=["cp", test_dir + "/system.toml", self.system_conf],
            stdouterr="copy_system_config",
        )

        # Copy dummy init system file to /etc/tedge
        copy_dummy_init_file = self.startProcess(
            command=self.sudo,
            arguments=["cp", test_dir + "/dummy_init.sh", self.dummy_init],
            stdouterr="copy_dummy_init",
        )

        self.addCleanupFunction(self.custom_cleanup)

    def execute(self):
        # Run tedge connect
        tedge_connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="tedge_connect",
            expectedExitStatus="==0",
        )

        # Run tedge disconnect
        tedge_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="tedge_disconnect",
            expectedExitStatus="==0",
        )

    def validate(self):
        # check there is no error
        self.assertGrep("tedge_connect.out", "Error", contains=False)
        self.assertGrep("tedge_disconnect.out", "Error", contains=False)
        self.assertGrep(self.dummy_init_output, "Error", contains=False)

    def custom_cleanup(self):
        # Remove system.toml from /etc/tedge, otherwise other tests will use the config.
        remove_system_config_file = self.startProcess(
            command=self.sudo,
            arguments=["rm", self.system_conf],
            stdouterr="remove_system_config",
        )

        # Copy dummy init system file to /etc/tedge
        copy_dummy_init_file = self.startProcess(
            command=self.sudo,
            arguments=["rm", self.dummy_init],
            stdouterr="remove_dummy_init",
        )

        # Remove the output from dummy_init
        copy_dummy_init_file = self.startProcess(
            command=self.sudo,
            arguments=["rm", self.dummy_init_output],
            stdouterr="remove_dummy_init_output",
        )

