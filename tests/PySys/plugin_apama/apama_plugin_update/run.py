import sys

from environment_apama_plugin import ApamaPlugin

"""
Validate apama plugin install command.
Using the apama project zip file in this test's Input directory as the test artefact.
This project deploys a mon file named "TedgeDemoMonitor" into the apama correlator.
"""


class ApamaPluginUpdateTest(ApamaPlugin):
    def setup(self):
        super().setup()
        # Assert that an apama project is not installed on the machine before test
        self.assert_project_installed(negate=True, abortOnError=True)

        # Install the project to be updated by this test
        self.install_project(self.project.apama_input_dir + "/quickstart.zip")
        self.wait_till_correlator_ready()
        self.assert_project_installed(abortOnError=True)
        self.assert_monitor_installed("TedgeDemoMonitor")

    def execute(self):
        # Use apama plugin `install` command to update the existing project with the archive in the shared input directory
        self.startProcess(
            command=self.sudo,
            arguments=[
                self.apama_plugin,
                "install",
                "LimitedBandwidth::project",
                "--file",
                self.project.apama_input_dir + "/limitedbandwidth.zip",
            ],
            stdouterr="plugin_install",
        )
        self.wait_till_correlator_ready()

    def validate(self):
        self.assert_project_installed()
        self.assert_monitor_installed("TedgeDemoMonitor", negate=True)
        self.assert_monitor_installed("ThinEdgeIoExample")
