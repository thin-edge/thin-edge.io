import sys
import os
import time

sys.path.append("apt_plugin")
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


class AptPluginFinalize(AptPlugin):
    def setup(self):
        super().setup()

        self.package = "squirrel3"
        self.dependency = "libsquirrel3-0"

        self.plugin_cmd("remove", "outp_remove", 0, self.package)
        self.plugin_cmd("remove", "outp_remove", 0, self.dependency)
        self.assert_isinstalled(self.package, False)
        self.assert_isinstalled_automatic(self.dependency, False)
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):
        self.plugin_cmd("install", "outp_install", 0, self.package)
        self.assert_isinstalled(self.package, True)
        self.assert_isinstalled_automatic(self.dependency, True)
        self.plugin_cmd("remove", "outp_remove", 0, self.package)
        self.assert_isinstalled(self.package, False)
        self.assert_isinstalled_automatic(self.dependency, True)
        self.plugin_cmd("finalize", "outp_finalize", 0)

    def validate(self):
        self.assert_isinstalled(self.package, False)
        self.assert_isinstalled_automatic(self.dependency, False)

    def cleanup_prepare(self):
        pass
