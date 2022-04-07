import os
from pysys.basetest import BaseTest


class ApamaPlugin(BaseTest):

    # Static class member that can be overriden by a command line argument
    # E.g.:
    # pysys.py run 'apt_*' -XmyPlatform='container'
    myPlatform = None
    apama_port = 15903
    apama_plugin = "/etc/tedge/sm-plugins/apama"
    apama_env_cmd = "/opt/softwareag/Apama/bin/apama_env"
    apama_dir = "/etc/tedge/apama"
    apama_project_dir = "/etc/tedge/apama/project"
    tmp_apama_dir = "/tmp/tedge_apama_project"
    sudo = "/usr/bin/sudo"

    def setup(self):
        if self.myPlatform != "container":
            self.skipTest(
                "Apama plugin tests are disabled by default. Execute `pysys run` with `-XmyPlatform=container` to run these tests"
            )

        # Register routines to cleanup containers and images added during test
        self.addCleanupFunction(self.cleanup_project)

    def wait_till_correlator_ready(self, timeout=5):
        """Wait till apama correlator is running and ready"""
        self.startProcess(
            command=self.sudo,
            arguments=[
                self.apama_env_cmd,
                "component_management",
                "--port",
                str(self.apama_port),
                "--waitFor",
                str(timeout),
            ],
            stdouterr="apama_correlator_wait",
        )

    def assert_monitor_installed(self, monitor_name, negate=False, abortOnError=False):
        """Asserts that a monitor with the given name is loaded in apama correlator"""
        process = self.startProcess(
            command=self.sudo,
            arguments=[self.apama_env_cmd, "engine_inspect", "-m", "-r"],
            stdouterr="apama_list_monitors",
        )

        self.assertGrep(
            process.stdout, monitor_name, contains=not negate, abortOnError=abortOnError
        )

    def assert_project_installed(self, negate=False, abortOnError=False):
        """Asserts that an apama project is installed or not"""
        self.assertPathExists(
            self.apama_project_dir, exists=not negate, abortOnError=abortOnError
        )

    def install_project(self, archive_path):
        """Install apama project from the provided archive."""
        self.startProcess(
            command=self.sudo,
            arguments=["mkdir", "-p", self.apama_dir],
            stdouterr="create_project_dir",
        )

        self.startProcess(
            command=self.sudo,
            arguments=["unzip", archive_path, "-d", self.tmp_apama_dir],
            stdouterr="unzip_apama_project",
        )

        self.startProcess(
            command=self.sudo,
            arguments=["mv", self.tmp_apama_dir + "/project", self.apama_project_dir],
            stdouterr="move_apama_project",
        )

        self.startProcess(
            command=self.sudo,
            arguments=["service", "apama", "restart"],
            stdouterr="restart_apama_service",
        )

    def remove_project(self):
        """Remove the installed apama project, if one is found."""
        if os.path.exists(self.apama_project_dir):
            self.log.info(f"Removing apama project")

            self.startProcess(
                command=self.sudo,
                arguments=["rm", "-rf", "/etc/tedge/apama/project"],
                stdouterr="remove_apama_project",
            )

            self.startProcess(
                command=self.sudo,
                arguments=["service", "apama", "stop"],
                stdouterr="stop_apama_service",
            )

    def assert_apama_service_running(self, negate=False):
        exitStatusExpr = "!=0" if negate else "==0"
        self.startProcess(
            command=self.sudo,
            arguments=["service", "apama", "status"],
            stdouterr="apama_service_status",
            expectedExitStatus=exitStatusExpr,
        )

    def plugin_install(self, project_archive_path):
        """Use apama plugin `install` command to install a project from the provided archive."""
        self.startProcess(
            command=self.sudo,
            arguments=[
                self.apama_plugin,
                "install",
                "project",
                "--file",
                project_archive_path,
            ],
            stdouterr="plugin_install",
        )

    def plugin_remove(self):
        """Use apama plugin `remove` command to remove the project, if one is installed already."""
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "remove", "project"],
            stdouterr="plugin_remove",
        )

    def plugin_finalize(self):
        """Use apama plugin `finalize` command to cleanup any stale files."""
        self.startProcess(
            command=self.sudo,
            arguments=[self.apama_plugin, "finalize"],
            stdouterr="plugin_finalize",
        )

    def cleanup_project(self):
        self.remove_project()
        if os.path.exists(self.tmp_apama_dir):
            self.startProcess(
                command=self.sudo,
                arguments=["rm", "-rf", "/tmp/tedge_apama_project"],
                stdouterr="remove_apama_tmp_dir",
            )
