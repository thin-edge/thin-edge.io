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

    def wait_if_restarting_mosquitto_too_fast(self):
        """Make sure we do not restart mosqiotto too fast
        Systemd will become suspicios when whe restart faster than 5 seconds
        """
        minimum_time = 10
        etimes = subprocess.check_output(
            "/usr/bin/ps -o etimes $(pidof mosquitto)", shell=True
        )
        runtime = int(etimes.split()[1])
        if runtime <= minimum_time:
            self.log.info(
                f"Restarting mosquitto too fast in the last {minimum_time} seconds. It was only up for {runtime} seconds"
            )
            # Derive additional delay time and add one safety second
            delay = minimum_time - runtime + 1
            self.log.info(f"Delaying execution by {delay} seconds")
            time.sleep(delay)

    def tedge_connect_c8y(self, expectedExitStatus="==0"):

        self.wait_if_restarting_mosquitto_too_fast()

        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="tedge_connect_c8y",
            expectedExitStatus=expectedExitStatus,
        )
        return connect

    def tedge_disconnect_c8y(self, expectedExitStatus="==0"):

        self.wait_if_restarting_mosquitto_too_fast()

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
