import sys
import os
sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin
"""
Validate apt plugin install from local file AND with a version - SUCCESS case

Using `rolldice` package from `_ROLLDICE_URL` bellow
"""


class AptPluginInstallFromLocalFileWithVersion(AptPlugin):
    """
    Testing that `apt` in `/etc/tedge/sm-plugins` can install from local file and check module version

    tedge command: 
        /etc/tedge/sm-plugins/apt install rolldice --file /path/to/file --module-version some_version
    """

    _ROLLDICE_URL = "http://ftp.br.debian.org/debian/pool/main/r/rolldice/rolldice_1.16-1+b3_amd64.deb"
    _path_to_rolldice_binary = None
    _module_version = "1.16-1+b3"  # NOTE: version here is given as a default arg because `_ROLLDICE_URL` is static

    def setup(self):
        super().setup()
        self._download_rolldice_binary(url=self._ROLLDICE_URL)          # downloading the binary
        self.addCleanupFunction(self.cleanup_remove_rolldice_binary)    # adding cleanup function to remove the binary
        self.apt_remove("rolldice")                                     # removing just in case rolldice is already on the machine
        self.assert_isinstalled("rolldice", False)                      # asserting previous step worked

    def execute(self):
        """
        executing command: /etc/tedge/apt install rolldice --file `self._path_to_rolldice_binary` --module-version `{self._module_version}`

        this should return exit_code = 0 (success)
        """
        self.plugin_cmd(
                command="install",
                outputfile="outp_install", 
                exit_code=0, 
                argument="rolldice", 
                file_path=f"{self._path_to_rolldice_binary}", 
                version=f"{self._module_version}")

    def validate(self):
        """
        checking that module `rolldice` is installed
        """
        self.assert_isinstalled("rolldice", True)

    def cleanup_remove_rolldice_binary(self):
        """
        if we have changed the value of `_path_to_rolldice_binary` from None, then the binary was successfully downloaded in 
        ``self.setup()``, so we call os to remove it
        """
        if self._path_to_rolldice_binary:
            os.remove(self._path_to_rolldice_binary)

