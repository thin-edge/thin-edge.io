import os
import subprocess
import time

from pysys.basetest import BaseTest


class TedgeEnvironment(BaseTest):
    """Class with helper and convenicence methods for testing tedge"""

    def setup(self):
        self.sudo = "/usr/bin/sudo"
        self.tedge = "/usr/bin/tedge"
        self.tedge_mapper_c8y = "tedge-mapper-c8y"
        self.sudo = "/usr/bin/sudo"
        self.systemctl = "/usr/bin/systemctl"

    def wait_if_restarting_mosquitto_too_frequently(self):
        """Make sure we do not restart mosqiotto too frequently
        Systemd will become suspicious when mosquitto is restarted more
        freqeuntly than 5 seconds.
        """
        # Ideally we would expect 5 seconds here, but only waiting
        # for 10 makes the issue diappear
        minimum_time = 10

        # Make sure we use the right path for ps, there seems to be an
        # issue with injecting PATH in pysys, so we use an absolute path for now
        if os.path.exists("/usr/bin/ps"):
            path_ps = "/usr/bin/ps"
        elif os.path.exists("/bin/ps"):
            # the place where mythic beasts has the ps
            path_ps = "/bin/ps"
        else:
            raise SystemError("Cannot find ps")

        etimes = subprocess.check_output(
            f"{path_ps} -o etimes $(pidof mosquitto)", shell=True
        )
        runtime = int(etimes.split()[1])
        if runtime <= minimum_time:
            self.log.info(
                f"Restarting mosquitto too frequently in the last {minimum_time} seconds. It was only up for {runtime} seconds"
            )
            # Derive additional delay time and add one safety second
            delay = minimum_time - runtime + 1
            self.log.info(f"Delaying execution by {delay} seconds")
            time.sleep(delay)

    def tedge_connect_c8y(self, expectedExitStatus="==0"):

        self.wait_if_restarting_mosquitto_too_frequently()

        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="tedge_connect_c8y",
            expectedExitStatus=expectedExitStatus,
        )
        return connect

    def tedge_disconnect_c8y(self, expectedExitStatus="==0"):

        self.wait_if_restarting_mosquitto_too_frequently()

        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="tedge_disconnect_c8y",
            expectedExitStatus=expectedExitStatus,
        )
        return connect

    def tedge_connect_c8y_test(self, expectedExitStatus="==0"):
        connect_c8y = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y", "--test"],
            stdouterr="tedge_connect_c8y_test",
            expectedExitStatus=expectedExitStatus,
        )
        return connect_c8y
