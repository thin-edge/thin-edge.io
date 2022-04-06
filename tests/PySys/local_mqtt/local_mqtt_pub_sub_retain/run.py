from pysys.basetest import BaseTest

import time
import os

"""
Validate local publishing with retain flag and subscribing:

Given a configured system
When we publish two messages with retain flag on to test/alarms/temp_sensor
When we start tedge sub in the background that subscribes to test/alarms/temp_sensor topic
When we publish an empty message with retain flag on to test/alarms/temp_sensor
Then we kill the subscriber
Now publish an empty message with retain flag on to test/alarms/temp_sensor
Start tedge sub in the background that subscribes to test/alarms/temp_sensor topic
Then we kill the subscriber
Then verify that there are retained message in the output of the first subscriber log
No empty message is present in the second subscriber log
"""


class MqttPublishWithRetain(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"
        self.environ = {"HOME": os.environ.get("HOME")}

    def execute(self):

        pub_to_set_alarm = self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "--retain",
                "--qos",
                "1",
                "test/alarms/temp_sensor",
                "alarm msg 1",
            ],
            stdouterr="pub_to_set_alarm",
            environs=self.environ,
        )

        pub_to_set_alarm = self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "--retain",
                "--qos",
                "1",
                "test/alarms/temp_sensor",
                "alarm msg 2",
            ],
            stdouterr="pub_to_set_alarm",
            environs=self.environ,
        )

        # wait some time before starting subscribers
        time.sleep(1)

        temp_sensor_sub = self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "sub", "test/alarms/#"],
            stdouterr="temp_sensor_sub",
            background=True,
            environs=self.environ,
        )

        pub_to_clear_alarm = self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "--retain",
                "--qos",
                "1",
                "test/alarms/temp_sensor",
                "",
            ],
            stdouterr="pub_to_clear_alarm",
            environs=self.environ,
        )

        # wait for a while before killing the subscribers
        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

        # Publish an empty message and start the subscriber, Now the subscriber will not receive any
        # message since it is an empty message.

        pub_to_clear_alarm = self.startProcess(
            command=self.tedge,
            arguments=[
                "mqtt",
                "pub",
                "--retain",
                "--qos",
                "1",
                "test/alarms/temp_sensor",
                "",
            ],
            stdouterr="pub_to_clear_alarm",
            environs=self.environ,
        )

        temp_sensor_empty_message = self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "sub", "test/alarms/#"],
            stdouterr="temp_sensor_empty_message",
            background=True,
            environs=self.environ,
        )

        # wait for a while before killing the subscribers
        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
            environs=self.environ,
        )

    def validate(self):
        self.assertGrep(
            "temp_sensor_sub.out",
            "\[test/alarms/temp_sensor\] alarm msg 2",
            contains=True,
        )
        self.assertGrep(
            "temp_sensor_sub.out", "\[test/alarms/temp_sensor\] ", contains=True
        )
        self.assertGrep(
            "temp_sensor_empty_message.out",
            "\[test/alarms/temp_sensor\] ",
            contains=False,
        )
