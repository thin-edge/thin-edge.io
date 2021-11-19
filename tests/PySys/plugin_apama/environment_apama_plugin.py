import os
from pysys.basetest import BaseTest


class ApamaPlugin(BaseTest):

    # Static class member that can be overriden by a command line argument
    # E.g.:
    # pysys.py run 'apt_*' -XmyPlatform='container'
    myPlatform = None
    apama_plugin = "/etc/tedge/sm-plugins/apama"
    apama_env_cmd = "/opt/softwareag/Apama/bin/apama_env"
    apama_project_dir = "/etc/tedge/apama/project"
    tmp_apama_dir = "/tmp/apama_project"
    sudo = "/usr/bin/sudo"

    def setup(self):
        if self.myPlatform != 'container':
            self.skipTest(
                'Testing the apama plugin is not supported on this platform')

        # Register routines to cleanup containers and images added during test
        self.addCleanupFunction(self.cleanup_project)

    def assert_monitor_installed(self, monitor_name, negate=False, abortOnError=False):
        """Asserts that a monitor with the given name is loaded in apama correlator"""
        process = self.startProcess(
            command=self.sudo,
            arguments=[self.apama_env_cmd, "engine_inspect", "-m", "-r"],
            stdouterr="apama_list_monitors"
        )

        self.assertGrep(
            process.stdout, monitor_name, contains=not negate, abortOnError=abortOnError)

    def assert_project_installed(self, negate=False, abortOnError=False):
        """Asserts that an apama project is installed or not"""
        self.assertPathExists(self.apama_project_dir,
                              exists=not negate, abortOnError=abortOnError)

    def install_project(self, archive_path):
        """Install apama project from the provided archive."""
        self.startProcess(
            command=self.sudo,
            arguments=["mkdir", "-p", self.apama_project_dir],
            stdouterr="create_project_dir"
        )

        self.startProcess(
            command=self.sudo,
            arguments=["unzip", archive_path, "-d", self.tmp_apama_dir],
            stdouterr="unzip_apama_project"
        )

        self.startProcess(
            command=self.sudo,
            arguments=["mv", self.tmp_apama_dir, self.apama_project_dir],
            stdouterr="move_apama_project"
        )

        self.startProcess(
            command=self.sudo,
            arguments=["service", "apama", "restart"],
            stdouterr="restart_apama_service"
        )

    def remove_project(self):
        """Remove the installed apama project, if one is found.
        """
        if os.path.exists(self.apama_project_dir):
            self.log.info(
                f"Removing apama project")

            self.startProcess(
                command=self.sudo,
                arguments=["rm", "-rf", self.apama_project_dir],
                stdouterr="remove_apama_project"
            )

            self.startProcess(
                command=self.sudo,
                arguments=["service", "apama", "stop"],
                stdouterr="stop_apama_service"
            )

    def plugin_install(self, project_archive_path):
        """Use apama plugin `install` command to install a project from the provided archive.
        """
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "install",
                       "project", project_archive_path],
            stdouterr="plugin_install"
        )

    def plugin_remove(self):
        """Use apama plugin `install` command to install a project from the provided archive.
        """
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "remove", "project"],
            stdouterr="plugin_remove"
        )

    def plugin_finalize(self):
        """Use apama plugin `finalize` command to cleanup any stale files.
        """
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "finalize"],
            stdouterr="plugin_finalize"
        )

    def cleanup_project(self):
        self.remove_project()
        if os.path.exists(self.tmp_apama_dir):
            self.startProcess(
                command=self.sudo,
                arguments=["rm", "-rf", self.tmp_apama_dir],
                stdouterr="remove_apama_tmp_dir"
            )
