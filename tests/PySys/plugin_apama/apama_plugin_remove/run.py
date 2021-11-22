import sys

from environment_apama_plugin import ApamaPlugin

"""
Validate apama plugin install command.
Using the apama project zip file in this test's Input directory as the test artefact.
This project deploys a mon file named "TedgeDemoMonitor" into the apama correlator.
"""


class ApamaPluginRemoveTest(ApamaPlugin):

    def setup(self):
        super().setup()
        # Assert that an apama project is not installed on the machine before test
        self.assert_project_installed(negate=True)

        # Install the project to be removed by this test
        self.install_project(self.input + "/quickstart.zip")
        self.assert_project_installed(abortOnError=True)

    def execute(self):
        self.remove_project()

    def validate(self):
        # Assert project NOT installed
        self.assert_project_installed(negate=True)
