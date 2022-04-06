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
        self.assert_project_installed(negate=True, abortOnError=True)

        # Install the project to be removed by this test
        self.install_project(self.project.apama_input_dir + "/quickstart.zip")
        self.assert_project_installed(abortOnError=True)

    def execute(self):
        # Use apama plugin `remove` command to remove the project, if one is installed already.
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "remove", "QuickStart::project"],
            stdouterr="plugin_remove",
        )

    def validate(self):
        # Assert project NOT installed
        self.assert_project_installed(negate=True)
