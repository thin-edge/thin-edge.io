from pysys.basetest import BaseTest

import time

"""
Validate local publishing with retain flag and subscribing:

Given a configured system
When we publish a message with retain flag to temperature sensor alaram topic (simulate set alaram)
When we publish an empty message with retain flag to pressure sensor alarm topic (simulate clear alarm)
When we start tedge sub in the background to subscribe to tempearture sensor alarm topic
When we start tedge sub in the background to subscribe to pressure sensor alarm topic
Then we kill both the subscribers
Then we find the retained message in the output of temperature subscriber log
And no message in the pressure sensor subscriber log
"""


class MqttPublishWithRetain(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        self.addCleanupFunction(self.retain_cleanup)

    def execute(self):   

        pub_to_set_alarm = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "--retain", "--qos", "1", "tedge/alarms/temp_sensor", "set temp alaram"],
            stdouterr="pub_to_set_alarm",
        )

        pub_to_clear_alarm = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "--retain", "--qos", "1", "tedge/alarms/pressure_sensor", ""],
            stdouterr="pub_to_clear_alarm",
        )
       
        # wait some time before starting subscribers
        time.sleep(1)

        temp_sensor_sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "tedge/alarms/temp_sensor"],
            stdouterr="temp_sensor_sub",
            background=True,
        )

        pres_sensor_sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "tedge/alarms/pressure_sensor"],
            stdouterr="pres_sensor_sub",
            background=True,
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
     
    def validate(self):
        self.assertGrep("temp_sensor_sub.out", "\[tedge/alarms/temp_sensor\] set temp alaram", contains=True)
        self.assertGrep("pres_sensor_sub.out", "\[tedge/alarms/pressure_sensor\]", contains=False)

    # clear retain message
    def retain_cleanup(self):
        pub_to_set_alarm = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "--retain", "--qos", "1", "tedge/alarms/temp_sensor", ""],
            stdouterr="pub_to_set_alarm",
        )