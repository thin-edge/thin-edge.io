from pysys.basetest import BaseTest

import time

"""
Validate message size that is published on tedge/measurements,
by subscribing for error message on tedge/errors by tedge-mapper c8y

Given a configured system
When we start the tedge-mapper-c8y systemctl service
When we start tedge sub with sudo in the background for error messages
When we publish a bigger message
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the error message in the output of tedge sub
Then we stop the tedge-mapper-c8y systemctl service

"""


class TedgeMapperC8yThresholdPacketSize(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"
        self.mosquitto_pub = "/usr/bin/mosquitto_pub"

        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "start", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper_c8y",
        )

        self.addCleanupFunction(self.mapper_cleanup)

    def execute(self):
        sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "--no-topic", "tedge/errors"],
            stdouterr="tedge_sub",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize. This is a heuristic measure.
        # Without an additional wait we observe failures in 1% of the test
        # runs.
        time.sleep(0.1)

        # Create a big file using the `dd` command
        msg = self.startProcess(
            command=self.sudo,
            arguments=[
                "dd",
                "if=/dev/zero",
                "of=/tmp/big_message.txt",
                "bs=10",
                "count=1",
                "seek=2048",
            ],
            stdouterr="tedge_msg",
        )

        pub = self.startProcess(
            command=self.sudo,
            arguments=[
                self.mosquitto_pub,
                "-t",
                "tedge/measurements",
                "-f",
                "/tmp/big_message.txt",
            ],
            stdouterr="tedge_pub",
        )

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertGrep(
            "tedge_sub.out",
            "The size of the message received on tedge/measurements is 20489 "
            # Note: 2**16 would bei 18384 but the C8y limit is a bit smaller
            + "which is greater than the threshold size of 16184.",
            contains=True,
        )

    def mapper_cleanup(self):
        self.log.info("mapper_cleanup")
        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper_c8y",
        )
