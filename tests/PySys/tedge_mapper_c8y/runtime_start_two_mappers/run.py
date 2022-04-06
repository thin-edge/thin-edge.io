from pysys.basetest import BaseTest


"""
Validate tedge_mapper can't be started twice.


Given unconnected system

When we start first tedge_mapper it runs uninterupted
When we start second tedge_mapper it doesn't connect to broker and exits with code 1 and logs error message

Then we validate output appropriate error message

"""


class RuntimeMultiMappers(BaseTest):
    def execute(self):
        tedge_mapper = "/usr/bin/tedge_mapper"
        self.sudo = "/usr/bin/sudo"

        tedge_mapper1 = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "start", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper1",
        )

        self.wait(0.1)

        tedge_mapper2 = self.startProcess(
            command=self.sudo,
            arguments=["-u", "tedge-mapper", tedge_mapper, "c8y"],
            stdouterr="tedge_mapper2",
            expectedExitStatus="==1",
        )

        stop = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-mapper-c8y"],
            stdouterr="tedge_mapper1",
        )

    def validate(self):
        self.assertGrep(
            "tedge_mapper2.err", "Error: Couldn't acquire file lock.", contains=True
        )
