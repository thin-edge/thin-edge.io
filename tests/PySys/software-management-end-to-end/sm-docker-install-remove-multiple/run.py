
from pysys.basetest import BaseTest

import time

"""
Validate end to end behaviour for the apt plugin for packages with version

When we install a package
Then it is installed
When we deinstall it again with the wrong version
Then it is still installed
When we deinstall it again with the right version
Then it is not installed

"""

import json
import requests
import time
import sys

sys.path.append("software-management-end-to-end")
from environment_sm_management import SoftwareManagement

class SMDockerInstallRemove(SoftwareManagement):

    def setup(self):
        super().setup()

        setup_action = [
            {
                "action": "install",
                "id": self.get_pkgid("hello-world"),
                "name": "hello-world",
                "url": " ",
                "version": "::docker",
            },
            {
                "action": "install",
                "id": self.get_pkgid("registry"),
                "name": "registry",
                "url": " ",
                "version": "2.6.2::docker",
            },
            {
                "action": "install",
                "id": self.get_pkgid("docker/getting-started"),
                "name": "docker/getting-started",
                "url": " ",
                "version": "::docker",
            },
        ]

        if self.dockerplugin != "dockerplugin":
            self.skipTest(
                "Testing the docker plugin is not supported on this platform")

        # self.trigger_action_json(setup_action)
        # self.wait_until_succcess()
        self.assertThat("False == value",
                        value=self.check_is_installed("hello-world"))
        self.assertThat("False == value",
                        value=self.check_is_installed("registry"))
        self.assertThat("False == value",
                        value=self.check_is_installed("docker/getting-started"))
        self.addCleanupFunction(self.docker_cleanup)

    def execute(self):

        execute_action = [
            {
                "action": "install",
                "id": self.get_pkgid("hello-world"),
                "name": "hello-world",
                "url": " ",
                "version": "::docker",
            },
            {
                "action": "install",
                "id": self.get_pkgid("registry"),
                "name": "registry",
                "url": " ",
                "version": "2.7.1::docker",
            },
            {
                "action": "install",
                "id": self.get_pkgid("docker/getting-started"),
                "name": "docker/getting-started",
                "url": " ",
                "version": "::docker",
            },
        ]

        self.trigger_action_json(execute_action)
        self.wait_until_succcess()

    def validate(self):

        self.assertThat("False == value",
                        value=self.check_is_installed("hello-world"))
        self.assertThat("True == value",
                        value=self.check_is_installed("registry"))
        self.assertThat("True == value",
                        value=self.check_is_installed("docker/getting-started"))

    def docker_cleanup(self):

        cleanup_action = [
            {
                "action": "remove",
                "id": self.get_pkgid("hello-world"),
                "name": "hello-world",
                "url": " ",
                "version": "::docker",
            },
            {
                "action": "remove",
                "id": self.get_pkgid("registry"),
                "name": "registry",
                "url": " ",
                "version": "2.6.2::docker",
            },
            {
                "action": "remove",
                "id": self.get_pkgid("docker/getting-started"),
                "name": "docker/getting-started",
                "url": " ",
                "version": "::docker",
            },
        ]

        self.trigger_action_json(cleanup_action)
        self.wait_until_succcess()

        self.assertThat("False == value",
                        value=self.check_is_installed("hello-world"))
        self.assertThat("False == value",
                        value=self.check_is_installed("registry"))
        self.assertThat("False == value",
                        value=self.check_is_installed("docker/getting-started"))
