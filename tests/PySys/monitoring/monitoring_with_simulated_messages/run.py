from pysys.basetest import BaseTest

import time

"""
Validate tedge-dm-agent  messages that are published
on tedge/measurements

Given a configured system
When we start the tedge-dm-agent with sudo in the background
When we start tedge sub with sudo in the background
When we start two publishers to publish the simulated collectd messages
Wait for couple of seconds to publish couple of batch of messages
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the  messages in the output of tedge sub,

"""


class MonitoringWithSimulatedMessages(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        collectd_mapper = self.startProcess(
            command=sudo,
            arguments=["systemctl", "start", "tedge-dm-agent"],
            stdouterr="collectd_mapper",
        )

        sub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "sub", "tedge/#"],
            stdouterr="tedge_sub",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize. This is a heuristic measure.
        # Without an additional wait we observe failures in 1% of the test
        # runs.
        time.sleep(0.1)

        pub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "pub",
                       "collectd/host/temperature/temp", "123435445:25.5"],
            stdouterr="tedge_temp",
        )

        pub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "pub",
                       "collectd/host/pressure/pres", "12345678:500.5"],
            stdouterr="tedge_pres",
        )

        # wait for tedge-dm-agent to batch messages
        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=sudo,
            arguments=["kill", "-9", str(sub.pid)],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertGrep("tedge_sub.out", "temperature")
        self.assertGrep("tedge_sub.out", "temp")
        self.assertGrep("tedge_sub.out", "25.5")
        self.assertGrep("tedge_sub.out", "pressure")
        self.assertGrep("tedge_sub.out", "pres")
        self.assertGrep("tedge_sub.out", "500.5")
