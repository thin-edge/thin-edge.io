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
        # Use apama plugin `install` command to install a project with the archive in the shared input directory
        self.startProcess(
            command=self.sudo,
            arguments=[
                self.apama_plugin,
                "install",
                "QuickStart::project",
                "--file",
                self.project.apama_input_dir + "/quickstart.zip",
            ],
            stdouterr="plugin_install",
        )
        self.wait_till_correlator_ready()
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "list"],
            stdouterr="plugin_list_after_install",
        )

    def validate(self):
        self.assert_project_installed()
        self.assert_monitor_installed("TedgeDemoMonitor")
        self.assertGrep("plugin_list_after_install.out", "tedge-demo-test::project")
        self.assertGrep("plugin_list_after_install.out", "TedgeDemoMonitor::mon")
