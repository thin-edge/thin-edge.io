import pysys
from pysys.basetest import BaseTest


class AptPlugin(BaseTest):
    def setup(self):
        self.apt_plugin = "/etc/tedge/sm-plugins/apt"
        self.apt_get = "/usr/bin/apt-get"
        self.sudo = "/usr/bin/sudo"

    def plugin_cmd(self, command, outputfile, exit_code, argument=None):
        args = [self.apt_plugin, command]
        if argument:
            args.append(argument)

        process = self.startProcess(
            command=self.sudo,
            arguments=args,
            stdouterr=outputfile,
            expectedExitStatus=f"=={exit_code}",
        )

    def apt_remove(self, package):
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, "remove", "-y", package],
            abortOnError=False,
        )
