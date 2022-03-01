from pysys.basetest import BaseTest

import time

from environment_c8y import EnvironmentC8y

"""
Validate an invalid JSON message that is published
on tedge/errors by tedge-mapper c8y

Given a configured system
When we start the tedge-mapper-c8y systemctl service
When we start tedge sub with sudo in the background
When we publish an invalid Thin Edge JSON message
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the error message in the output of tedge sub
Then we stop the tedge-mapper-c8y systemctl service

"""


class TedgeMapperC8y(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

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

        pub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub",
                       "tedge/measurements", "{"],
            stdouterr="tedge_temp",
        )

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertGrep('tedge_sub.out', "Invalid JSON", contains=True)

    def mapper_cleanup(self):
        self.log.info("mapper_cleanup")
        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper_c8y",
        )
