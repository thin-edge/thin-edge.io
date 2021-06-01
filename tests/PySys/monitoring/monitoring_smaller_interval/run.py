from pysys.basetest import BaseTest

import time
import json

"""
Validate tedge-dm-agent  messages that are published
on tedge/measurements

Given a configured system
When we start the tedge-dm-agent with sudo in the background
When we start tedge sub with sudo in the background
When we start two publishers to publish the simulated collectd messages
Publish the messages in 100ms interval
Wait for couple of seconds to publish couple of batch of messages
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the  messages in the output of tedge sub,

"""

class MonitoringSmallInterval(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        collectd_mapper = self.startProcess(
            command=sudo,
            arguments=["systemctl", "start", "tedge-dm-agent"],
            stdouterr="collectd_mapper",
        )

        sub = self.startProcess(
            command=sudo,
            arguments=[tedge, "mqtt", "sub", "--no-topic", "tedge/#"],
            stdouterr="tedge_sub",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize. This is a heuristic measure.
        # Without an additional wait we observe failures in 1% of the test
        # runs.
        time.sleep(0.1)

        for i in range(10) :

            pub = self.startProcess(
                command=sudo,
                arguments=[tedge, "mqtt", "pub",
                           "collectd/host/temperature/temp", "123435445:25.5"],
                stdouterr="tedge_temp",
            )

            pub = self.startProcess(
                command=sudo,
                arguments=[tedge, "mqtt", "pub",
                           "collectd/host/pressure/pres", "12345678:500.5"],
                stdouterr="tedge_pres",
            )

            #publish every 100ms
            time.sleep(0.1)

        # wait for tedge-dm-agent to batch messages
        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertThat('collectd_msg_validation_result == expected_result',
                        collectd_msg_validation_result=self.validate_json(), expected_result=True)

    def validate_json(self):
        f = open(self.output + '/tedge_sub.out', 'r')
        lines = f.readlines()
        for line in lines:
            self.js_msg = json.loads(line)
            if not self.validate_time():
                return False
            if not self.validate_temperature():
                return False
            if not self.validate_pressure():
                return False
        return True 

    def validate_time(self):
        if self.js_msg["time"]:
            return True
        else:
            return False

    def validate_temperature(self):
        if self.js_msg["temperature"]:
            if "temp" in self.js_msg["temperature"]:
                return True
            else:
                return False
        else:
            return False

    def validate_pressure(self):
        if self.js_msg["pressure"]:
            if "pres" in self.js_msg["pressure"]:
                return True
            else:
                return False
        else:
            return False
