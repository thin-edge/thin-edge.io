from pysys.basetest import BaseTest

import time
import json

"""
Validate a tedge-mapper-az message without timestamp that is published
on az/messages/events/

Given a configured system
When we set az.mapper.timestamp false
When we start the tedge-mapper-az systemctl service
When we start tedge sub with sudo in the background
When we publish a Thin Edge JSON message
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the JSON message in the output of tedge sub
Then we stop the tedge-mapper-az systemctl service
Then we unset az.mapper.timestamp

"""


class TedgeMapperAzWithoutTimestamp(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        tedge_config = self.startProcess(
            command=self.sudo,
            arguments=["tedge", "config", "set", "az.mapper.timestamp", "false"],
            stdouterr="tedge_temp",
        )

        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "start", "tedge-mapper-az"],
            stdouterr="tedge_mapper_az",
        )

        self.addCleanupFunction(self.mapper_cleanup)

    def execute(self):
        sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "--no-topic", "az/messages/events/"],
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
            arguments=[
                self.tedge,
                "mqtt",
                "pub",
                "tedge/measurements",
                '{"temperature": 12}',
            ],
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
        f = open(self.output + "/tedge_sub.out", "r")
        thin_edge_json = json.load(f)

        self.assertThat(
            "actual == expected", actual=thin_edge_json["temperature"], expected=12
        )
        self.assertGrep("tedge_sub.out", "time", contains=False)

    def mapper_cleanup(self):
        self.log.info("mapper_cleanup")
        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-mapper-az"],
            stdouterr="tedge_mapper_az",
        )

        tedge_config = self.startProcess(
            command=self.sudo,
            arguments=["tedge", "config", "unset", "az.mapper.timestamp"],
            stdouterr="tedge_temp",
        )
