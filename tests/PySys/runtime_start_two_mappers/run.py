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
            arguments=["-u", "tedge-mapper", tedge_mapper],
            stdouterr="tedge_mapper1",
            expectedExitStatus="==0",
            background=True
        )

        self.wait(0.5)

        tedge_mapper2 = self.startProcess(
            command=self.sudo,
            arguments=["-u", "tedge-mapper", tedge_mapper],
            stdouterr="tedge_mapper2",
            expectedExitStatus="==1",
        )

        # since the first mapper is running with different user rights the
        # test runner can't kill it for us. So we need to kill it ourselves
        kill = self.startProcess(
            command=self.sudo,
            arguments=["sh", "-c", "kill -9 $(pgrep -x tedge_mapper)"],
            stdouterr="kill",
            ignoreExitStatus=True
            )

    def validate(self):
        self.assertGrep("tedge_mapper2.err", "Error: Couldn't acquire file lock.", contains=True)
