import sys
import time
import subprocess

from pysys.basetest import BaseTest

"""
Validate the tedge-mapper-sm-c8y does not loose last message from tedge-agent when it fails and comes back

Given a configured system
When `rolldice` package is installed
when a subscriber is started to `sudo tedge mqtt sub 'c8y/s/us'`
When tedge_agent is started as `sudo systemctl start tedge-agent.service`
When sm mapper is started as `sudo systemctl start tedge-mapper-sm-c8y.service`
When send a delete operation `sudo tedge mqtt pub "c8y/s/ds" "528,tedge,rolldice,,,delete"`
When sm mapper is stopped `sudo systemctl stop tedge-mapper-sm-c8y.service`
Wait for sometime for operation to be completed
When sm mapper is restarted `sudo systemctl restart tedge-mapper-sm-c8y.service`
Now sm mapper receives the last update result message, process and forward it to the cloud on `c8y/s/us`
Then validate subscriber output for `501,c8y_SoftwareUpdate`, for the status of operation
Then validate subscriber output for `503,c8y_SoftwareUpdate` for final result of operation
Then test has passed
"""

class SmMapperC8yReceiveLastMessageOnRestart(BaseTest):
    systemctl = "/usr/bin/systemctl"
    tedge = "/usr/bin/tedge"
    sudo = "/usr/bin/sudo"
    apt = "/usr/bin/apt-get"
   
    def setup(self):
        self.startProcess(
            command=self.sudo,
            arguments=[self.apt, "install", "rolldice"],
            stdouterr="install",
        )

        self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "c8y/s/us"],
            stdouterr="tedge_sub",
            background=True,
        )

        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "tedge-agent.service"],
            stdouterr="tedge_agent_start",
        )
        
        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "tedge-mapper-sm-c8y.service"],
            stdouterr="sm_mapper_start",
        )

        self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "c8y/s/ds", "528,tedge,rolldice,,,delete"],
            stdouterr="tedge_pub",           
        )

    def execute(self):
        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "tedge-mapper-sm-c8y.service"],
            stdouterr="sm_mapper_stop",
        )

        # wait for the operation to complete
        time.sleep(15)

        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "restart", "tedge-mapper-sm-c8y.service"],
            stdouterr="sm_mapper_restart",
        )

        # wait for the sm mapper to process and publish result to cloud
        # and subscriber to capture the output and log it.
        time.sleep(10)

        # Stop the subscriber
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="kill_out",
        )

    def validate(self):
        self.log.info("Validate")
        self.assertGrep("tedge_sub.out", "501,c8y_SoftwareUpdate", contains=True)
        self.assertGrep("tedge_sub.out", "503,c8y_SoftwareUpdate", contains=True)
    