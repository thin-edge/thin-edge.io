import sys
import os
import time

sys.path.append("apt-plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin prepare
(As far as currently possible)

When we check that squirrel is not installed
When we check that libsquirrel is not installed
When we install squirrel
When we make sure that squirrel and libsquirrel are installed
When we remove the package squirrel
When we make sure that squirrel is not and is libsquirrel installed
When we call finalize
Then we make sure that squirrel and libsquirrel are not installed

We use package squirrel3 as is a command-line program and it depends only on on libsquirrel.

"""


class AptPluginInstallTest(AptPlugin):
    def setup(self):
        super().setup()
        self.plugin_cmd("remove", "outp_remove", 0, "squirrel3")
        self.plugin_cmd("remove", "outp_remove", 0, "libsquirrel3-0")
        self.assert_isinstalled("squirrel3", False)
        self.assert_isinstalled_automatic("libsquirrel3-0", False)
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):
        self.plugin_cmd("install", "outp_install", 0, "squirrel3")
        self.assert_isinstalled("squirrel3", True)
        self.assert_isinstalled_automatic("libsquirrel3-0", True)
        self.plugin_cmd("remove", "outp_remove", 0, "squirrel3")
        self.assert_isinstalled("squirrel3", False)
        self.assert_isinstalled_automatic("libsquirrel3-0", True)
        self.plugin_cmd("finalize", "outp_finalize", 0)

    def validate(self):
        self.assert_isinstalled("squirrel3", False)
        self.assert_isinstalled_automatic("libsquirrel3-0", False)

    def cleanup_prepare(self):
        pass
