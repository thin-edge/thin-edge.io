from pysys.basetest import BaseTest

"""
Validate local publishing while no mosquitto is running

Given a configured system
When we stop mosquitto
When we publish something with qos 2
Then we expect an error code
Then we restart mosquitto
"""


class PySysTest(BaseTest):
    def setup(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"
        self.systemctl = "/usr/bin/systemctl"

        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "mosquitto"],
            stdouterr="stop",
        )

        self.addCleanupFunction(self.mycleanup)

    def execute(self):

        pub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "--qos", "2", "mqtt", "pub", "atopic", "amessage"],
            stdouterr="tedge_pub_fail",
            expectedExitStatus="==1",
        )

        # validate exit status with the expected status from calling startProcess
        self.assertThat("value" + pub.expectedExitStatus, value=pub.exitStatus)

    def mycleanup(self):
        self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "mosquitto"],
            stdouterr="start",
        )
