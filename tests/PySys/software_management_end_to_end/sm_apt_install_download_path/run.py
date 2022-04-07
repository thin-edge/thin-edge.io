import os
import subprocess

from environment_sm_management import SoftwareManagement
from environment_tedge import TedgeEnvironment
from retry import retry

"""
This test checks that the install action downloads the package (rolldice.deb) to the location
specified by tedge config set tmp.path <some value>

steps:

    1. set tmp.path to /tedge_download_path_test
    2. reconnect c8y
    3. trigger donload
    4. assert package downloads in /tedge_download_path_test
    5. remove package
    6. reset tmp.path to initial value
"""


@retry(Exception, tries=10, delay=0.5)
def assert_install_in_download_path(install_directory):
    """
    assert rolldice is installed in correct path
    """
    assert "rolldice" in os.listdir(f"{install_directory}")


class AptInstallWithDownloadPath(SoftwareManagement, TedgeEnvironment):
    SUDO = "/usr/bin/sudo"
    TEDGE = "/usr/bin/tedge"
    DOWNLOAD_DIR = "/tedge_download_path_test"
    CURRENT_DOWNLOAD_PATH = None

    def reconnect_c8y(self):
        self.tedge_disconnect_c8y()
        self.tedge_connect_c8y()

    def set_download_path(self, download_path):
        self.startProcess(
            command=self.SUDO,
            arguments=[self.TEDGE, "config", "set", "tmp.path", f"{download_path}"],
        )

    def setup(self):
        super().setup()
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

        # creating directory
        self.startProcess(
            command=self.SUDO, arguments=["mkdir", f"{self.DOWNLOAD_DIR}"]
        )
        self.startProcess(
            command=self.SUDO, arguments=["chmod", "a+rwx", f"{self.DOWNLOAD_DIR}"]
        )

        self.CURRENT_DOWNLOAD_PATH = (
            subprocess.check_output(
                f"{self.SUDO} {self.TEDGE} config get tmp.path", shell=True
            )
            .decode("utf8")
            .strip()
        )

        # setting download.path
        self.set_download_path(self.DOWNLOAD_DIR)

        self.reconnect_c8y()

    def execute(self):

        self.trigger_action(
            package_name="rolldice",
            package_id=self.get_pkgid("rolldice"),
            version="::apt",
            url="https://thin-edge-io.eu-latest.cumulocity.com/inventory/binaries/12435463",
            action="install",
        )

        # download path validation
        assert_install_in_download_path(install_directory=self.DOWNLOAD_DIR)

        self.wait_until_succcess()

        self.assertThat("True == value", value=self.check_is_installed("rolldice"))

        self.trigger_action(
            package_name="rolldice",
            package_id=self.get_pkgid("rolldice"),
            version="",
            url="",
            action="delete",
        )

        self.wait_until_succcess()

    def validate(self):
        self.assertThat("False == value", value=self.check_is_installed("rolldice"))

    def mysmcleanup(self):

        self.startProcess(
            command=self.SUDO, arguments=["rm", "-rf", f"{self.DOWNLOAD_DIR}"]
        )

        self.set_download_path(self.CURRENT_DOWNLOAD_PATH)

        # reconnect is required to revert back to initial download config
        self.reconnect_c8y()
