import sys

from environment_apama_plugin import ApamaPlugin

"""
Validate apama plugin's monitor support.
Using TedgeTestMonitor mon file in this test's Input directory as the test artefact.
This project installs and removes this mon file into the apama correlator using the apama plugin.
"""


class ApamaPluginMonSupportTest(ApamaPlugin):
    def setup(self):
        super().setup()
        # Assert that an apama project is not installed on the machine before test
        self.assert_project_installed(negate=True)

        # Install a project to be updated by this test
        self.install_project(self.project.apama_input_dir + "/quickstart.zip")
        self.wait_till_correlator_ready()

        # Assert that the monitor injected by this test does not exist already
        self.assert_monitor_installed("TedgeTestMonitor", negate=True)

    def execute(self):
        # Use apama plugin `install` command to install a project with the archive in the shared input directory
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "list"],
            stdouterr="plugin_list_before_install",
        )

        self.startProcess(
            command=self.sudo,
            arguments=[
                self.apama_plugin,
                "install",
                "TedgeTestMonitor::mon",
                "--file",
                self.project.apama_input_dir + "/TedgeTestMonitor.mon",
            ],
            stdouterr="plugin_install",
        )
        self.wait_till_correlator_ready()

        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "list"],
            stdouterr="plugin_list_after_install",
        )

        self.startProcess(
            command=self.sudo,
            arguments=[
                self.apama_plugin,
                "install",
                "TedgeTestMonitor::mon",
                "--file",
                self.project.apama_input_dir + "/TedgeTestMonitorV2.mon",
            ],
            stdouterr="plugin_update",
        )
        self.wait_till_correlator_ready()

        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "list"],
            stdouterr="plugin_list_after_update",
        )

        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "remove", "TedgeTestMonitor::mon"],
            stdouterr="plugin_remove",
        )

        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "list"],
            stdouterr="plugin_list_after_remove",
        )

    def validate(self):
        self.assertGrep(
            "plugin_list_before_install.out", "TedgeTestMonitor", contains=False
        )
        self.assertGrep("plugin_list_after_install.out", "TedgeTestMonitor")
        self.assertGrep("plugin_list_after_update.out", "TedgeTestMonitor")
        self.assertGrep(
            "plugin_list_after_remove.out", "TedgeTestMonitor", contains=False
        )
