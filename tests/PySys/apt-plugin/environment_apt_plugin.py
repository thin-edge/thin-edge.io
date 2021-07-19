import pysys
from pysys.basetest import BaseTest


class AptPlugin(BaseTest):
    def setup(self):
        self.apt_plugin = "/etc/tedge/sm-plugins/apt"
        self.apt_get = "/usr/bin/apt-get"
        self.sudo = "/usr/bin/sudo"

        self.list_calls = 0

    def plugin_cmd(self, command, outputfile, exit_code, argument=None):
        """Call a plugin with command and an optional argument,
        expect exit code and store output to outputfile
        """
        args = [self.apt_plugin, command]
        if argument:
            args.append(argument)

        process = self.startProcess(
            command=self.sudo,
            arguments=args,
            stdouterr=outputfile,
            expectedExitStatus=f"=={exit_code}",
        )

    def assert_isinstalled(self, package, state):
        """Asserts that a package is installed or not"""
        self.plugin_cmd("list", f"outp_check_{self.list_calls}", 0)
        self.assertGrep(f"outp_check_{self.list_calls}.out", package, contains=state)
        self.list_calls += 1

    def apt_remove(self, package):
        """Use apt to remove a package.
        Added so that we can avoid to use the code under test for maintenance.
        """
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, "remove", "-y", package],
            abortOnError=False,
        )
