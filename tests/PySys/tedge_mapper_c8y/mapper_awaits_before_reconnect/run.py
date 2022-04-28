from pysys.basetest import BaseTest
from datetime import datetime, timedelta

"""
Validate mapper doesn't reconnect too often.


Given unconnected system

When we start mosquitto_sub to use fixed MQTT Client ID
When we start tedge_mapper systemd service to use fixed MQTT Client ID
When we observe tedge_mapper tries to reconnect

Then we wait for 5 seconds
Then we validate output contains no more than 5 error messages

"""


class MapperReconnectAwait(BaseTest):
    def execute(self):
        tedge_mapper = "/usr/bin/tedge_mapper"
        self.sudo = "/usr/bin/sudo"

        subber = self.startProcess(
            command="/usr/bin/mosquitto_sub",
            arguments=["-i", "tedge-mapper-c8y", "-t", "test"],
            stdouterr="mosquitto_sub",
            background=True,
        )

        self.wait(0.1)

        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "start", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper",
        )

        self.wait(5)

        mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper",
        )

    def validate(self):
        dt = datetime.now()
        dt_end = dt.strftime("%Y-%m-%d %H:%M:%S")
        dt_start = (dt - timedelta(seconds=5)).strftime("%Y-%m-%d %H:%M:%S")

        journal = self.startProcess(
            command=self.sudo,
            arguments=[
                "journalctl",
                "-u",
                "tedge-mapper-c8y.service",
                "--since",
                dt_start,
                "--until",
                dt_end,
            ],
            stdouterr="tedge_mapper_journal",
        )

        self.assertGrep("tedge_mapper_journal.out", "ERROR", contains=True)
        self.assertLineCount(
            "tedge_mapper_journal.out",
            condition="<=5",
            abortOnError=True,
            expr="tedge_mapper::core::mapper: MQTT connection error:",
        )
        self.assertLineCount(
            "tedge_mapper_journal.out",
            condition=">=2",
            abortOnError=True,
            expr="tedge_mapper::core::mapper: MQTT connection error:",
        )
