from pysys.basetest import BaseTest

import time
import json
import sys

from environment_c8y import EnvironmentC8y
#sys.path.append("environments")

"""
Validate a tedge-mapper-c8y message that is published
on c8y/measurement/measurements/create

Given a configured system
When we start the tedge-mapper-c8y systemctl service
When we start tedge sub with sudo in the background
When we publish a Thin Edge JSON message
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the JSON message in the output of tedge sub
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
            arguments=[self.tedge, "mqtt", "sub", "--no-topic", "c8y/measurement/measurements/create"],
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
                       "tedge/measurements", '{"temperature": 12, "time": "2021-06-15T17:01:15.806181503+02:00"}'],
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
        f = open(self.output + '/tedge_sub.out', 'r')
        c8y_json = json.load(f)

        self.assertThat('actual == expected', actual = c8y_json['type'], expected = 'ThinEdgeMeasurement')
        self.assertThat('actual == expected', actual = c8y_json['temperature']['temperature']['value'], expected = 12)
        self.assertThat('actual == expected', actual = c8y_json['time'], expected = '2021-06-15T17:01:15.806181503+02:00')

    def mapper_cleanup(self):
        self.log.info("mapper_cleanup")
        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper_c8y",
        )
