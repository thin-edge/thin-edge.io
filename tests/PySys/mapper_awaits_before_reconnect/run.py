from pysys.basetest import BaseTest


"""
Validate mapper doesn't reconnect too often.


Given unconnected system

When we start mosquitto_sub to use fixed MQTT Client ID
When we start tedge_mapper to use fixed MQTT Client ID
When we observe tedge_mapper tries to reconnect

Then we wait for 5 seconds
Then we validate output contains no more than 5 error messages

"""


class MapperReconnectAwait(BaseTest):
    def execute(self):
        tedge_mapper = "/usr/bin/tedge_mapper"
        self.sudo = "/usr/bin/sudo"


        subber = self.startProcess(
            command="/usr/bin/mosquitto_sub",
            arguments=["-i", "tedge-mapper", "-t", "test"],
            stdouterr="mosquitto_sub",
            background=True
        )

        self.wait(0.1)

        mapper = self.startProcess(
            command=self.sudo,
            arguments=["-u", "tedge-mapper", tedge_mapper],
            stdouterr="tedge_mapper",
            background=True
        )

        self.wait(5)

        # since the first mapper is running with different user rights the
        # test runner can't kill it for us. So we need to kill it ourselves
        kill = self.startProcess(
            command=self.sudo,
            arguments=["sh", "-c", "kill -9 $(pgrep -x tedge_mapper)"],
            stdouterr="kill",
            ignoreExitStatus=True
            )

    def validate(self):
        self.assertGrep("tedge_mapper.out", "ERROR", contains=True)
        self.assertLineCount("tedge_mapper.out", condition='<=5', abortOnError=True, expr='tedge_mapper::mapper: MQTT connection error:')
        self.assertLineCount("tedge_mapper.out", condition='>=4', abortOnError=True, expr='tedge_mapper::mapper: MQTT connection error:')
