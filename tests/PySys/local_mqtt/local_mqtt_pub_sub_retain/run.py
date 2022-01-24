from pysys.basetest import BaseTest

import time

"""
Validate local publishing with retain flag and subscribing:

Given a configured system
When we start tedge pub with sudo to publish a message with retain flag to temperature sensor alaram topic
When we start tedge pub with sudo to publish an empty message with retain flag to pressure sensor alarm topic
When we start tedge sub with sudo in the background to subscribe to tempearture sensor alarm topic
hen we start tedge sub with sudo in the background to subscribe to pressure sensor alarm topic
Then we kill tedge sub with sudo as it is running with a different user account
Then we find the messages in the output of temperature and pressure sensor subscribers logs
"""


class MqttPublishWithRetain(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        pub_to_set_alarm = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "pub", "--retain", "--qos", "1", "tedge/alarms/temp_sensor", "set temp alaram"],
            stdouterr="pub_to_set_alarm",
        )

        pub_to_clear_alarm = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "pub", "--retain", "--qos", "1", "tedge/alarms/pressure_sensor", ""],
            stdouterr="pub_to_clear_alarm",
        )
       
        # wait some time before starting subscribers
        time.sleep(1)

        temp_sensor_sub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "sub", "tedge/alarms/temp_sensor"],
            stdouterr="temp_sensor_sub",
            background=True,
        )

        pres_sensor_sub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "sub", "tedge/alarms/pressure_sensor"],
            stdouterr="pres_sensor_sub",
            background=True,
        )

        # wait for a while before killing the subscribers
        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )
     
    def validate(self):
        self.assertGrep("temp_sensor_sub.out", "\[tedge/alarms/temp_sensor\] set temp alaram", contains=True)
        self.assertGrep("pres_sensor_sub.out", "\[tedge/alarms/pressure_sensor\]", contains=False)
