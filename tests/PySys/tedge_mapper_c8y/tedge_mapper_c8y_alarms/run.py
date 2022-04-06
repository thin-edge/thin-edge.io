from environment_c8y import EnvironmentC8y

import time
import json
import os

"""
Validate alarms published from thin-edge

When some alarms are raised by publishing to local MQTT bus,
corresponding alarms with the same data are raised in C8Y.
Clearning an alarm on the local bus results in clearing the same alarm in C8Y as well.
"""


class TedgeMapperC8yAlarm(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.addCleanupFunction(self.test_cleanup)

    def execute(self):
        # Publish one temperature_high alarm with "WARNING" severity to thin-edge device

        # Pysys seems to print and record the environment it will also print passwords in the env
        # Solution: only inject the variables we really need
        environ = {"HOME": os.environ.get("HOME")}

        self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "-r",
                "tedge/alarms/warning/temperature_high",
                '{"text":"temperature is high", "time":"2021-12-15T15:22:06.464247777+05:30"}',
            ],
            environs=environ,
        )

        # Publish one temperature_high alarm with "MAJOR" severity to thin-edge device
        self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "-r",
                "tedge/alarms/major/temperature_very_high",
                '{"text":"temperature is very high"}',
            ],
            environs=environ,
        )

        # Publish one temperature_high alarm with "CRITICAL" severity to thin-edge device
        self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "-r",
                "tedge/alarms/critical/temperature_dangerous",
                '{"text":"temperature is dangerously high"}',
            ],
            environs=environ,
        )

        # Publish one temperature_high alarm with "MINOR" severity to thin-edge device
        self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "-r",
                "tedge/alarms/minor/temperature_low",
                '{"text":"temperature low"}',
            ],
            environs=environ,
        )

        # Clear the last "MINOR" alarm
        self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "pub", "-r", "tedge/alarms/minor/temperature_low", ""],
            environs=environ,
        )

        # Waiting for the mapped measurement message to reach the Cloud
        time.sleep(1)

    def validate(self):
        # Even though this call retrieves the last 3 alarms,
        # the last minor alarm won't be included as it's already cleared
        alarms = self.cumulocity.get_last_n_alarms_from_device(self.project.deviceid, 3)

        # Validate the last 3 measurements of thin-edge-child device
        self.validate_alarm(
            alarms[0],
            "temperature_dangerous",
            "CRITICAL",
            "temperature is dangerously high",
        )
        self.validate_alarm(
            alarms[1], "temperature_very_high", "MAJOR", "temperature is very high"
        )
        self.validate_alarm(
            alarms[2],
            "temperature_high",
            "WARNING",
            "temperature is high",
            "2021-12-15T15:22:06.464247777+00:30",
        )

    def validate_alarm(
        self,
        alarm_json,
        expected_alarm_type,
        expected_alarm_severity,
        expected_alarm_text=None,
        expected_alarm_time=None,
    ):
        self.log.info(json.dumps(alarm_json, indent=4))
        self.assertThat(
            "actual == expected",
            actual=alarm_json["type"],
            expected=expected_alarm_type,
        )
        self.assertThat(
            "actual == expected",
            actual=alarm_json["severity"],
            expected=expected_alarm_severity,
        )
        self.assertThat(
            "actual == expected",
            actual=alarm_json["text"],
            expected=expected_alarm_text,
        )
        if expected_alarm_time:
            self.assertThat(
                "actual == expected",
                actual=alarm_json["type"],
                expected=expected_alarm_type,
            )

    def test_cleanup(self):
        # Clear all previously raised alarms
        self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "-r",
                "tedge/alarms/warning/temperature_high",
                "",
            ],
        )
        self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "-r",
                "tedge/alarms/major/temperature_very_high",
                "",
            ],
        )
        self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "-r",
                "tedge/alarms/critical/temperature_dangerous",
                "",
            ],
        )

        self.cumulocity.clear_all_alarms_from_device(self.project.deviceid)
