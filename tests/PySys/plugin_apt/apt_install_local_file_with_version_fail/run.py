import sys
import os

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin install from local file AND with a version - FAIL case

Using `rolldice` package from `rolldice_url` bellow
"""


class AptPluginInstallFromLocalFileWithVersionFail(AptPlugin):
    """
    Testing that `apt` in `/etc/tedge/sm-plugins` install returns exit code 5 (Internal Error)
    when a wrong module-version is provided
    """

    _path_to_rolldice_binary = None
    _module_version = "nonsense"

    def setup(self):
        super().setup()
        self._download_rolldice_binary(
            url=self.get_rolldice_package_url()
        )  # downloading the binary
        self.addCleanupFunction(
            self.cleanup_remove_rolldice_binary
        )  # adding cleanup function to remove the binary
        self.apt_remove(
            "rolldice"
        )  # removing just in case rolldice is already on the machine
        self.assert_isinstalled("rolldice", False)  # asserting previous step worked

    def execute(self):
        """
        executing command: /etc/tedge/apt install rolldice --file `self._path_to_rolldice_binary` --module-version `{self._module_version}`

        this should return exit_code = 5 (internal error)
        """
        self.plugin_cmd(
            command="install",
            outputfile="outp_install",
            exit_code=5,
            argument="rolldice",
            file_path=f"{self._path_to_rolldice_binary}",
            version=f"{self._module_version}",
        )

    def validate(self):
        """
        checking that module `rolldice` is installed
        """
        self.assert_isinstalled("rolldice", False)

    def cleanup_remove_rolldice_binary(self):
        """
        if we have changed the value of `_path_to_rolldice_binary` from None, then the binary was successfully downloaded in
        ``self.setup()``, so we call os to remove it
        """
        if self._path_to_rolldice_binary:
            os.remove(self._path_to_rolldice_binary)
