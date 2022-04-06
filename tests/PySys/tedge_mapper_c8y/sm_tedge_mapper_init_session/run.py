from pysys.basetest import BaseTest

"""
Validate tedge_mapper c8y init session feature.

Given unconnected system

When the system is connected to cumulocity cloud
Then stop the tedge-mapper-c8y
Then clean the tedge-mapper-c8y session that is present in the broker
Then initialize the tedge-mapper-c8y session and subscribing to the topics
Then publish a software list request onto c8y/s/us
Then start a subscriber to get the request 'tedge/commands/res/software/list'
Then start the tedge-mapper-c8y
Now the mapper should receive the request that was receive the previous request
    and forward the request to agent on 'tedge/commands/res/software/list'
Now stop the subscriber and validate the message received on 'tedge/commands/res/software/list'
Validate the response for that contains id.

"""
from environment_c8y import EnvironmentC8y


class TedgeSMMapperInitSession(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.sudo = "/usr/bin/sudo"
        self.tedge = "/usr/bin/tedge"
        self.systemctl = "/usr/bin/systemctl"
        self.tedge_mapper = "/usr/bin/tedge_mapper"

        self.addCleanupFunction(self.init_cleanup)

    def execute(self):

        stop_tedge_mapper = self.startProcess(
            command=self.sudo,
            arguments=["systemctl", "stop", "tedge-mapper-c8y"],
            stdouterr="stop_tedge_mapper",
        )

        remove_lock = self.startProcess(
            command=self.sudo,
            arguments=["rm", "-rf", "/var/lock/tedge-mapper-c8y.lock"],
            stdouterr="remove_lock",
        )
        mapper_drop = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge_mapper, "--clear", "c8y"],
            stdouterr="mapper_drop",
        )

        mapper_init = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge_mapper, "--init", "c8y"],
            stdouterr="mapper_init",
        )

        pub_req = self.startProcess(
            command=self.sudo,
            arguments=[
                self.tedge,
                "mqtt",
                "pub",
                "--qos",
                "1",
                "c8y/s/us",
                "118,software-management",
            ],
            stdouterr="pub_req",
        )

        sub_resp = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "sub", "tedge/commands/req/software/list"],
            stdouterr="sub_resp",
            background=True,
        )

        start_mapper = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "tedge-mapper-c8y"],
            stdouterr="start_mapper",
        )

        self.wait(2)

        sub_stop = self.startProcess(
            command=self.sudo,
            arguments=["killall", "tedge"],
            stdouterr="sub_stop",
        )

    def validate(self):
        self.assertGrep("sub_resp.out", "id", contains=True)

    def init_cleanup(self):
        super().myenvcleanup()
