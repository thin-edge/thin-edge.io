from pysys.basetest import BaseTest


"""
Validate tedge_agent can't be started twice.


Given unconnected system

When we start first tedge_agent it runs uninterupted
When we start second tedge_agent it doesn't connect to broker and exits with code 1 and logs error message

Then we validate output appropriate error message

"""


class RuntimeMultiAgents(BaseTest):
    def execute(self):
        tedge_agent = "/usr/bin/tedge_agent"
        self.sudo = "/usr/bin/sudo"

        tedge_agent1 = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "start", "tedge-agent"],
            stdouterr="tedge_agent1",
        )

        self.wait(1)

        tedge_agent2 = self.startProcess(
            command=self.sudo,
            arguments=["-u", "tedge-agent", tedge_agent],
            stdouterr="tedge_agent2",
            expectedExitStatus="==1",
        )

        stop = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-agent"],
            stdouterr="tedge_agent1",
        )

    def validate(self):
        self.assertGrep("tedge_agent2.err", "Error: Couldn't acquire file lock.", contains=True)
