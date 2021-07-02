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
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"
        systemctl = "/usr/bin/systemctl"

        self.startProcess(
            command=sudo,
            arguments=[systemctl, "stop", "mosquitto"],
            stdouterr="stop",
        )

        self.startProcess(
            command=sudo,
            arguments=[tedge, "--qos", "2", "mqtt", "pub", "atopic", "amessage"],
            stdouterr="tedge_pub_fail",
            expectedExitStatus="==1",
        )

        self.startProcess(
            command=sudo,
            arguments=[systemctl, "start", "mosquitto"],
            stdouterr="start",
        )
