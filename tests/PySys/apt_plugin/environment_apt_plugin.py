"""
This environment provides a basis for tests of the apt plugin.
Handle with care, these tests will install and remove packages.

The tests are disabled by default as they will install, de-install
packages, run apt update and more.

Better run them in a VM or a container.

To run the tests:

    pysys.py run 'apt_*' -XmyPlatform='container'

"""

import pysys
from pysys.basetest import BaseTest


class AptPlugin(BaseTest):

    # Static class member that can be overridden by a command line argument
    # E.g.:
    # pysys.py run 'apt_*' -XmyPlatform='container'
    myPlatform=None

    def setup(self):
        if self.myPlatform != 'container':
            self.skipTest('Testing the apt plugin is not supported on this platform')

        self.apt_plugin = "/etc/tedge/sm-plugins/apt"
        self.apt_get = "/usr/bin/apt-get"
        self.sudo = "/usr/bin/sudo"

        self.list_calls = 0
        self.list_calls_auto = 0

    def plugin_cmd(
        self, command, outputfile, exit_code, argument=None, version=None, extra=None
    ):
        """Call a plugin with command and an optional argument,
        expect exit code and store output to outputfile
        """
        args = [self.apt_plugin, command]
        if argument:
            args.append(argument)

        if version:
            args.append("--module-version")
            args.append(version)

        if extra:
            # Does not happen in normal cases, just for testing
            args.append(extra)

        process = self.startProcess(
            command=self.sudo,
            arguments=args,
            stdouterr=outputfile,
            expectedExitStatus=f"=={exit_code}",
        )

        self.assertThat("value" + process.expectedExitStatus, value=process.exitStatus)

    def assert_isinstalled(self, package, state):
        """Asserts that a package is installed or not"""
        self.plugin_cmd("list", f"outp_check_{self.list_calls}", 0)
        self.assertGrep(f"outp_check_{self.list_calls}.out", package, contains=state)
        self.list_calls += 1

    def assert_isinstalled_automatic(self, package, state):
        """Asserts that a package is installed or not"""
        if state:
            status = 0
        else:
            status = 1
        process = self.startProcess(
            command="/usr/bin/dpkg-query",
            arguments=["-s", package],
            stdouterr=f"outp_check_{self.list_calls_auto}",
            expectedExitStatus=f"=={status}",
        )
        self.list_calls_auto += 1

    def apt_remove(self, package):
        """Use apt to remove a package.
        Added so that we can avoid to use the code under test for maintenance.
        """
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, "remove", "-y", package],
            abortOnError=False,
        )

    def apt_install(self, package):
        """Use apt to install a package.
        Added so that we can avoid to use the code under test for maintenance.
        """
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt_get, "install", "-y", package],
            abortOnError=False,
        )
