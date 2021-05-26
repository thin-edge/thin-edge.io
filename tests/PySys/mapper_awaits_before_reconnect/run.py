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

        subber = self.startProcess(
            command="mosquito_sub",
            arguments=["-i", "tedge-mapper-c8y", "-t", "test"],
            stdouterr="mosquitto_sub",
            background=True
        )

        self.wait(0.1)

        mapper = self.startProcess(
            command=tedge_mapper,
            arguments=["c8y"],
            stdouterr="tedge_mapper",
            background=True
        )

        self.wait(5)

    def validate(self):
        self.assertGrep("tedge_mapper.out", "ERROR", contains=True)
        self.assertLineCount("tedge_mapper.out", condition='<=5', abortOnError=True, expr='tedge_mapper::mapper: MQTT connection error:')
        self.assertLineCount("tedge_mapper.out", condition='>=4', abortOnError=True, expr='tedge_mapper::mapper: MQTT connection error:')
