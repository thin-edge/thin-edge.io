import sys
import os

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin install from local file - FAIL case
"""


class AptPluginInstallFromLocalFileFail(AptPlugin):
    """
    Testing that `apt` in `/etc/tedge/sm-plugins` install returns exit code 5 (Internal Error)
    when a wrong file_path is provided
    """

    _path_to_rolldice_binary = None
    _fake_path_to_rolldice_binary = None

    def setup(self):
        super().setup()
        current_working_directory = os.path.abspath(os.getcwd())
        self._fake_path_to_rolldice_binary = os.path.join(
            current_working_directory, "notafile.deb"
        )
        self.apt_remove(
            "rolldice"
        )  # removing just in case rolldice is already on the machine
        self.assert_isinstalled("rolldice", False)  # asserting previous step worked

    def execute(self):
        """
        executing command: /etc/tedge/apt install rolldice --file `self._fake_path_to_rolldice_binary`

        this should return exit_code = 5 (internal error)
        """
        self.plugin_cmd(
            command="install",
            outputfile="outp_install",
            exit_code=5,
            argument="rolldice",
            file_path=f"{self._fake_path_to_rolldice_binary}",
        )

    def validate(self):
        """
        checking that module `rolldice` is NOT installed
        """
        self.assert_isinstalled("rolldice", False)
