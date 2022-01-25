from pysys.basetest import BaseTest

import time

"""
Validate local publishing with retain flag and subscribing:

Given a configured system
When we publish two messages with retain flag on to test/alarms/temp_sensor
When we start tedge sub in the background that subscribes to test/alarms/temp_sensor topic
When we publish an empty message with retain flag on to test/alarms/temp_sensor
Then we kill the subscriber
Then we find the retained message in the output of the subscriber log
"""


class MqttPublishWithRetain(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"
      
    def execute(self):
        
        pub_to_set_alarm = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "--retain", "--qos", "1", "test/alarms/temp_sensor", "alarm msg 1"],
            stdouterr="pub_to_set_alarm",
        )

        pub_to_set_alarm = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "--retain", "--qos", "1", "test/alarms/temp_sensor", "alarm msg 2"],
            stdouterr="pub_to_set_alarm",
        )
       
        # wait some time before starting subscribers
        time.sleep(1)

        temp_sensor_sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "test/alarms/temp_sensor"],
            stdouterr="temp_sensor_sub",
            background=True,
        )

        pub_to_clear_alarm = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "--retain", "--qos", "1", "test/alarms/temp_sensor", ""],
            stdouterr="pub_to_clear_alarm",
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
        self.assertGrep("temp_sensor_sub.out", "\[test/alarms/temp_sensor\] alarm msg 2", contains=True)
        self.assertGrep("temp_sensor_sub.out", "\[test/alarms/temp_sensor\] ", contains=True)
