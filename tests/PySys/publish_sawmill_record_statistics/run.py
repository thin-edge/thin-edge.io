import os
import sys

sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Publish sawmill and record process statistics

Given a configured system with configured certificate
When we derive from EnvironmentC8y
When we publish with the sawtooth_publisher with 100ms cycle time and publish
    6 times 100 values to the Sawmill topic (10 on each publish) (60s).
When we record the output of mosquittos $SYS/# topic
When we record the /proc/pid/status of mosquitto
When we record the /proc/pid/status of tedge-mapper
When we upload the data to the github action for further analysis (not done here)

TODO : Add validation procedure
"""


class PublishSawmillRecordStatistics(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        super().execute()
        self.log.info("Execute")

        sub = self.startProcess(
            command="/usr/bin/mosquitto_sub",
            arguments=["-v", "-h", "localhost", "-t", "$SYS/#"],
            stdouterr="mosquitto_sub_stdout",
            background=True,
        )

        # record /proc/pid/status

        status_mosquitto = self.startProcess(
            command="/bin/sh",
            arguments=[
                "-c",
                "while true; do date; cat /proc/$(pgrep -x mosquitto)/status; sleep 1; done",
            ],
            stdouterr="status_mosquitto_stdout",
            background=True,
        )

        status_mapper = self.startProcess(
            command="/bin/sh",
            arguments=[
                "-c",
                "while true; do date; cat /proc/$(pgrep -f -x \"/usr/bin/tedge_mapper c8y\")/status; sleep 1; done",
            ],
            stdouterr="status_mapper_stdout",
            background=True,
        )

        # record /proc/pid/stat

        stats_mapper = self.startProcess(
            command="/bin/sh",
            arguments=[
                "-c",
                "while true; do cat /proc/$(pgrep -f -x \"/usr/bin/tedge_mapper c8y\")/stat; sleep 1; done",
            ],
            stdouterr="stat_mapper_stdout",
            background=True,
        )

        stats_mosquitto = self.startProcess(
            command="/bin/sh",
            arguments=[
                "-c",
                "while true; do cat /proc/$(pgrep -x mosquitto)/stat; sleep 1; done",
            ],
            stdouterr="stat_mosquitto_stdout",
            background=True,
        )

        # record /proc/pid/statm

        statm_mapper = self.startProcess(
            command="/bin/sh",
            arguments=[
                "-c",
                "while true; do cat /proc/$(pgrep -f -x \"/usr/bin/tedge_mapper c8y\")/statm; sleep 1; done",
            ],
            stdouterr="statm_mapper_stdout",
            background=True,
        )

        statm_mosquitto = self.startProcess(
            command="/bin/sh",
            arguments=[
                "-c",
                "while true; do cat /proc/$(pgrep -x mosquitto)/statm; sleep 1; done",
            ],
            stdouterr="statm_mosquitto_stdout",
            background=True,
        )

        # start the publisher

        publisher = self.project.exampledir + "/sawtooth_publisher"
        cmd = os.path.expanduser(publisher)

        pub = self.startProcess(
            command=cmd,
            # run for one minute
            arguments=["100", "100", "6", "sawmill"],
            stdouterr="stdout_sawmill",
        )

    def validate(self):
        super().validate()

        # These are mostly placeholder validations to make sure
        # that the file is there and is at least not empty
        self.assertGrep('mosquitto_sub_stdout.out', 'mosquitto', contains=True)

        self.assertGrep('status_mapper_stdout.out', 'tedge_mapper', contains=True)
        self.assertGrep('status_mosquitto_stdout.out', 'mosquitto', contains=True)

        self.assertGrep('stat_mapper_stdout.out', 'tedge_mapper', contains=True)
        self.assertGrep('stat_mosquitto_stdout.out', 'mosquitto', contains=True)

        self.assertGrep('statm_mapper_stdout.out', expr=r'\d', contains=True)
        self.assertGrep('statm_mosquitto_stdout.out', expr=r'\d', contains=True)




    def mycleanup(self):
        self.log.info("My Cleanup")
