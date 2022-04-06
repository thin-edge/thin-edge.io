import os

from pysys.basetest import BaseTest

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
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # system config file must be located in /etc/tedge
        self.system_conf = "/etc/tedge/system.toml"

        self.tmp_dir = "/tmp/dummy_init"
        self.dummy_init = self.tmp_dir + "/dummy_init.sh"
        self.dummy_init_output = self.tmp_dir + "/dummy_init.out"

        test_dir = os.getcwd() + "/misc_features/custom_init_system"

        # Copy system.toml file to /etc/tedge
        copy_system_config_file = self.startProcess(
            command=self.sudo,
            arguments=["cp", test_dir + "/system.toml", self.system_conf],
            stdouterr="copy_system_config",
        )

        # Copy dummy init system file and directory to /tmp
        copy_dummy_init = self.startProcess(
            command=self.sudo,
            arguments=["cp", "-rfP", test_dir + "/dummy_init", "/tmp"],
            stdouterr="copy_dummy_init",
        )

        self.addCleanupFunction(self.custom_cleanup)

    def execute(self):
        # Run tedge connect
        tedge_connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="tedge_connect",
        )

        # Run tedge disconnect
        tedge_disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="tedge_disconnect",
        )

    def validate(self):
        # Check there is no error
        self.assertGrep("tedge_connect.out", "Error", contains=False)
        self.assertGrep("tedge_disconnect.out", "Error", contains=False)
        self.assertGrep(self.dummy_init_output, "Error", contains=False)
        self.assertGrep(
            self.dummy_init_output,
            "The system config file '/etc/tedge/system.toml' doesn't exist.",
            contains=False,
        )

        expected_output = [
            "is_available",
            "restart mosquitto",
            "enable mosquitto",
            "restart tedge-mapper-c8y",
            "enable tedge-mapper-c8y",
            "restart tedge-agent",
            "enable tedge-agent",
            "is-active mosquitto",
            "stop tedge-mapper-c8y",
            "disable tedge-mapper-c8y",
            "stop tedge-agent",
            "disable tedge-agent",
        ]

        # Check the output of init system contains all expected words
        for word in expected_output:
            self.assertGrep(self.dummy_init_output, word, contains=True)

    def custom_cleanup(self):
        # Remove system.toml from /etc/tedge, otherwise other tests will use the config.
        remove_system_config_file = self.startProcess(
            command=self.sudo,
            arguments=["rm", "-f", self.system_conf],
            stdouterr="remove_system_config",
        )

        # Remove dummy_init
        remove_dummy_init = self.startProcess(
            command=self.sudo,
            arguments=["rm", "-rf", self.tmp_dir],
            stdouterr="remove_dummy_init",
        )
