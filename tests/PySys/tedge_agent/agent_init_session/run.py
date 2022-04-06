from pysys.basetest import BaseTest
from datetime import datetime, timedelta

"""
Validate tedge agent init session feature.


Given unconnected system

When we start tedge_agent to initialize session and exit
When we start publish the software list request
When we start a subsciber to get the response for the request
When we start the tedge_agent, it gets the previous response and responds
Then stop the agent and the subscriber
Then validate the output of the subscriber for response
"""


class AgentInitSession(BaseTest):
    def setup(self):
        self.sudo = "/usr/bin/sudo"
        self.tedge_agent = "/usr/bin/tedge_agent"
        self.tedge = "/usr/bin/tedge"
        self.systemctl = "/usr/bin/systemctl"

        remove_lock = self.startProcess(
            command=self.sudo,
            arguments=["rm", "-rf", "/var/lock/tedge_agent.lock"],
            stdouterr="remove_lock",
        )

    def execute(self):
        agent_clear = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge_agent, "--clear"],
            stdouterr="agent_clear",
        )

        agent_init = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge_agent, "--init"],
            stdouterr="agent_init",
        )

        pub_req = self.startProcess(
            command=self.sudo,
            arguments=[
                self.tedge,
                "mqtt",
                "pub",
                "--qos",
                "1",
                "tedge/commands/req/software/list",
                '{"id":"Ld3KgqpcLDlrYH6sfpG7w"}',
            ],
            stdouterr="pub_req",
        )

        sub_resp = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "tedge/commands/res/software/list"],
            stdouterr="sub_resp",
            background=True,
        )

        agent_start = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "tedge-agent"],
            stdouterr="agent_start",
        )

        self.wait(2)

        agent_stop = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "tedge-agent"],
            stdouterr="agent_stop",
        )

        sub_stop = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="sub_stop",
        )

    def validate(self):
        self.assertGrep("sub_resp.out", "Ld3KgqpcLDlrYH6sfpG7w", contains=True)
