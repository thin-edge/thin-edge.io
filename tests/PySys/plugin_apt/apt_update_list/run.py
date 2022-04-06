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
"""


class AptPluginUpdateList(AptPlugin):

    update_list = "update-list"

    package1_name = "rolldice"
    package2_name = "asciijump"
    package3_name = "moon-buggy"

    def setup(self):
        super().setup()

        # Prepare the system under test with rolldice and moon-buggy not installed and asciijump installed,
        # so that the test can install rolldice and moon-buggy and remove asciijump
        self.apt_remove(self.package1_name)
        self.apt_install(self.package2_name)
        self.apt_remove(self.package3_name)

        # Validate that the system under test is in the expected state
        self.assert_isinstalled(self.package1_name, False)
        self.assert_isinstalled(self.package2_name, True)
        self.assert_isinstalled(self.package3_name, False)

        # Register cleanup function to remove all test packages after the test
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):

        # The 'update_list_input' file from the 'Input' directory contains the update instructions
        # This file has instructions to install rolldice and moon-buggy and remove asciijump

        # TODO The file contains OS specific content
        # TODO The version field is not accepted
        # ERROR: CSV error: record 1 (line: 2, byte: 27): found record with 2 fields, but the previous record has 3 fields
        # TODO Trailing tabs added
        # install	rolldice	1.16-1+b3
        input_list_path = f"{self.input}/update_list_input"

        # Execute the update-list command with the update instructions passed to its stdin from a file
        os.system(
            f"{self.sudo} {self.apt_plugin} {self.update_list} < {input_list_path }"
        )

    def validate(self):
        self.assert_isinstalled(self.package1_name, True)
        self.assert_isinstalled(self.package2_name, False)
        self.assert_isinstalled(self.package3_name, True)

    def cleanup_prepare(self):
        self.apt_remove(self.package1_name)
        self.apt_remove(self.package2_name)
        self.apt_remove(self.package3_name)
