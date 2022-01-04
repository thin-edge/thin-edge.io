import sys
import time
import subprocess
from pathlib import Path

from pysys.basetest import BaseTest

"""
Validate the tedge-mapper-sm-c8y does not loose last message from tedge-agent when it fails and comes back

TODO: Update this descripton

Pradeep:
This actually to test the stability of the sm mapper.
It will start get the request for updating the software, passes the request to agent and then dies.
Meanwhile the agent process the request and sends the response back to the broker, the message stays there till the mapper comes back again.
Once the mapper comes back/restarts the response will be received and forwarded that to the c8y


Given a configured system
When `rolldice` package is installed
when a subscriber is started as `sudo tedge mqtt sub 'c8y/s/us'`
When tedge_agent is started as `sudo systemctl start tedge-agent.service`
When sm mapper is started as `sudo systemctl start tedge-mapper-sm-c8y.service`
When send a delete operation `sudo tedge mqtt pub "c8y/s/ds" "528,tedge,rolldice,,,delete"`
When sm mapper is stopped `sudo systemctl stop tedge-mapper-sm-c8y.service`
Wait for sometime for operation to be completed and agent to push the operation result.
When sm mapper is restarted `sudo systemctl restart tedge-mapper-sm-c8y.service`
Now sm mapper receives the last update result message, process and forwards it to the cloud on `c8y/s/us`
Then validate subscriber output for `501,c8y_SoftwareUpdate`, for the status of operation
Then validate subscriber output for `503,c8y_SoftwareUpdate` for final result of operation
Then test has passed
"""

class SmMapperC8yReceiveLastMessageOnRestart(BaseTest):
    systemctl = "/usr/bin/systemctl"
    tedge = "/usr/bin/tedge"
    sudo = "/usr/bin/sudo"
    apt = "/usr/bin/apt-get"
    mqtt_sub = "/usr/bin/mosquitto_sub"
    rm = "/usr/bin/rm"
    def setup(self):

        self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="connect_c8y",
        )

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
            arguments=[self.tedge, "mqtt", "pub", "c8y/s/ds", "528,tedge,rolldice,,,delete"],
            stdouterr="tedge_pub",           
        )

        # Delete and now we wait, so there is a time depencency

        self.addCleanupFunction(self.smcleanup)

    def execute(self):
        self.log.info("Waiting for 10s")
        time.sleep(10)

        #import pdb; pdb.set_trace()

        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "tedge-mapper-sm-c8y.service"],
            stdouterr="sm_mapper_stop",
        )

        self.startProcess(
            command=self.mqtt_sub,
            arguments=["-v", "-t", "tedge/commands/res/software/update"],
            stdouterr="tedge_sub_agent",
            background=True,
        )

        # check if the agent has completed the operation
        self.log.info("Delaying for 15s until we check if the agent has completed the operation")
        time.sleep(15)
       
        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "restart", "tedge-mapper-sm-c8y.service"],
            stdouterr="sm_mapper_restart",
        )

        # wait for the sm mapper to process and publish result to cloud
        # and subscriber to capture the output and log it.
        self.log.info("Delaying for 30s")
        time.sleep(30)

        # Stop the subscriber
        kill = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge", "mosquitto_sub"],
            stdouterr="kill_out",
        )

    def validate(self):
        self.log.info("Validate")
        self.assertGrep("tedge_sub.out", "501,c8y_SoftwareUpdate", contains=True)
        self.assertGrep("tedge_sub.out", "503,c8y_SoftwareUpdate", contains=True)
    
    def smcleanup(self):
        self.log.info("Stop sm-mapper and agent")
        self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="connect_c8y",
        )

    def setup_mosquitto(self):
        raise SystemError("Make sure, nobody calls me ... ")
        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "mosquitto.service"],
            stdouterr="mosquitto_stop",
        )
        self.startProcess(
            command=self.sudo,
            arguments=[self.rm, "/var/lib/mosquitto/mosquitto.db"],
            stdouterr="remove_db",
        )
        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "restart", "mosquitto.service"],
            stdouterr="restart_mosquitto",
        )       
