from pysys.basetest import BaseTest

import time
import re
import json

"""
Validate tedge-dm-agent  messages that are published
on tedge/measurements

Given a configured system
When we start the collectd with sudo in the background
When we start the tedge-dm-agent with sudo in the background
When we start tedge sub with sudo in the background
Wait for couple of seconds to publish couple of batch of messages
Then we kill tedge sub with sudo as it is running with a different user account
Then we validate the  messages in the output of tedge sub,
"""


class MonitoringWithCollectd(BaseTest):
    js_msg = ""
    cpu_cnt = 0
    memory_cnt = 0
    time_cnt = 0
    disk_cnt = 0

    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        collectd = self.startProcess(
            command=sudo,
            arguments=["systemctl", "start", "collectd"],
            stdouterr="collectd",
        )

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
        # to initialize and capture couple of batches of messages
        # that are published by tedge-dm-agent.
        time.sleep(12)

        # Kill the subscriber process explicitly with sudo as PySys does
        # not have the rights to do it
        kill = self.startProcess(
            command=sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

    def validate(self):
        self.assertGrep("tedge_sub.out", r'time|cpu|memory|df-root')
        self.assertThat('collectd_msg_validation_result == expected_result',
                        collectd_msg_validation_result=self.validate_json(), expected_result=True)

    def validate_json(self):
        f = open(self.output + '/tedge_sub.out', 'r')
        lines = f.readlines()
        for line in lines:
            self.js_msg = json.loads(line)
            if not self.validate_cpu():
                return False
            if not self.validate_time():
                return False
            if not self.validate_memory():
                return False
            # validate disk stats if the entries are present, as the disk stats collection window is bigger
            if "df-root" in self.js_msg:
                self.validate_disk()
        if self.time_cnt == self.cpu_cnt == self.memory_cnt and self.disk_cnt > 0 and self.disk_cnt <= 3:
            return True
        else:
            return False

    def validate_cpu(self):
        if self.js_msg["cpu"]:
            if "percent-active" in self.js_msg["cpu"]:
                self.cpu_cnt += 1
                return True
            else:
                return False
        else:
            return False

    def validate_time(self):
        if self.js_msg["time"]:
            self.time_cnt += 1
            return True
        else:
            return False

    def validate_memory(self):
        if self.js_msg["memory"]:
            if "percent-used" in self.js_msg["memory"]:
                self.memory_cnt += 1
                return True
            else:
                return False
        else:
            return False

    def validate_disk(self):
        if "percent_bytes-used" in self.js_msg["df-root"]:
            self.disk_cnt += 1
            return True
        else:
            return False
