import sys

from environment_apama_plugin import ApamaPlugin

"""
Validate apama plugin install command.
Using the apama project zip file in this test's Input directory as the test artefact.
This project deploys a mon file named "TedgeDemoMonitor" into the apama correlator.
"""


class ApamaPluginInstallTest(ApamaPlugin):

    def setup(self):
        super().setup()
        # Assert that an apama project is not installed on the machine before test
        self.assert_project_installed(negate=True)

    def execute(self):
        self.plugin_install(self.input + "/quickstart.zip")

    def validate(self):
        self.assert_project_installed()
        self.assert_monitor_installed("TedgeDemoMonitor")
