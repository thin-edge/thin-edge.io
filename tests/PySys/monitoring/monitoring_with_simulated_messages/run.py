from pysys.basetest import BaseTest

import time
import json

"""
Validate collectd-mapper  messages that are published
on tedge/measurements

Given a configured system
When we start the collectd-mapper with sudo in the background
When we start tedge sub with sudo in the background
When we start two publishers to publish the simulated collectd messages
Wait for couple of seconds to publish couple of batch of messages
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the  messages in the output of tedge sub,

"""


class MonitoringWithSimulatedMessages(BaseTest):
    def setup(self):
        self.js_msg = ""
        self.time_cnt = 0
        self.temp_cnt = 0
        self.pres_cnt = 0
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # stop collectd to avoid mixup of  messages
        collectd = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "collectd"],
            stdouterr="collectd",
        )

        collectd_mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "start", "collectd-mapper"],
            stdouterr="collectd_mapper",
        )
        self.addCleanupFunction(self.monitoring_cleanup)

    def execute(self):
        sub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "--no-topic", "tedge/#"],
            stdouterr="tedge_sub",
            background=True,
        )

        # Wait for a small amount of time to give tedge sub time
        # to initialize. This is a heuristic measure.
        # Without an additional wait we observe failures in 1% of the test
        # runs.
        time.sleep(0.1)

        temp_pub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub",
                       "collectd/host/temperature/temp", "123435445:25.5"],
            stdouterr="tedge_temp",
        )

        pres_pub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub",
                       "collectd/host/pressure/pres", "12345678:500.5"],
            stdouterr="tedge_pres",
        )

        # wait for collectd-mapper to batch messages
        time.sleep(1)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=self.sudo,
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
                reason = "time validation failed in message: " + str(line)
                self.abort(False, reason)
            if "temperature" in self.js_msg:
                if not self.validate_temperature():
                    reason = "temperature stat validation failed in message: " + \
                        str(line)
                    self.abort(False, reason)
            if "pressure" in self.js_msg:
                if not self.validate_pressure():
                    reason = "pressure stat validation failed in message: " + \
                        str(line)
                    self.abort(False, reason)
        if self.time_cnt == 2 and self.temp_cnt == 1 and self.pres_cnt == 1:
            return True
        else:
            return False

    def validate_time(self):
        if self.js_msg["time"]:
            self.time_cnt += 1
            return True
        else:
            return False

    def validate_temperature(self):
        if self.js_msg["temperature"]:
            if "temp" in self.js_msg["temperature"]:
                self.temp_cnt += 1
                return True
            else:
                return False
        else:
            return False

    def validate_pressure(self):
        if self.js_msg["pressure"]:
            if "pres" in self.js_msg["pressure"]:
                self.pres_cnt += 1
                return True
            else:
                return False
        else:
            return False

    def monitoring_cleanup(self):
        self.log.info("monitoring_cleanup")
        collectd = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "collectd-mapper"],
            stdouterr="collectd_mapper",
        )
