import pysys
from pysys.basetest import BaseTest


class AptPlugin(BaseTest):
    def setup(self):
        self.apt_plugin = "/etc/tedge/sm-plugins/apt"
        self.apt_get = "/usr/bin/apt-get"
        self.sudo = "/usr/bin/sudo"

        # Generator function to diffenciate the output of
        # consecutive calls of isinstalled
        self.generator = self.newval()

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

    def newval(self):
        """Generator function that returns values 0..call_count"""
        val = 0
        while True:
            yield val
            val += 1

    def assert_isinstalled(self, package, state):
        """Asserts that a package is installed or not"""
        val = next(self.generator)
        self.plugin_cmd("list", f"outp_check_{val}", 0)
        self.assertGrep(f"outp_check_{val}.out", package, contains=state)

    def apt_remove(self, package):
        """Use apt to remove a package"""
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, "remove", "-y", package],
            abortOnError=False,
        )
