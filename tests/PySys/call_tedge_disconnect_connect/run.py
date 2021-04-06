import pysys
from pysys.basetest import BaseTest

import time

"""
Disconnect and connect

In the end this should be the other way round

TODO : describe procedure
"""

class PySysTest(BaseTest):
    def execute(self):
        tedge = "/usr/bin/tedge"
        sudo = "/usr/bin/sudo"

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["mosquitto", "status"],
            stdouterr="serv_mosq"
        )

        if serv_mosq.exitStatus!=0:
            self.log.error("The Mosquitto service is not running")
            self.abort(FAILED)

        # Check if tedge-mapper is active
        serv_mapper = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_mapper1"
        )

        if serv_mapper.exitStatus!=0:
            self.log.error("The tedge-mapper service is not running")
            self.abort(FAILED)


        disconnect = self.startProcess(
            command=sudo,
            arguments=[tedge, "disconnect", "c8y"],
            stdouterr="tedge_disconnect",
            expectedExitStatus='==0',
        )

        # Check if tedge-mapper is inactive
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_mapper2",
            expectedExitStatus='==3',
        )

        if serv_mosq.exitStatus!=3:
            self.log.error("The tedge-mapper service is running")
            self.abort(FAILED)

        connect = self.startProcess(
            command=sudo,
            arguments=[tedge, "connect", "c8y"],
            stdouterr="tedge_connect",
            expectedExitStatus='==0',
        )

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command="/usr/sbin/service",
            arguments=["mosquitto", "status"],
            stdouterr="serv_mosq2"
        )

        if serv_mosq.exitStatus!=0:
            self.log.error("The Mosquitto service is not running")
            self.abort(FAILED)

        # Check if tedge-mapper is active again
        serv_mapper = self.startProcess(
            command="/usr/sbin/service",
            arguments=["tedge-mapper", "status"],
            stdouterr="serv_mapper3"
        )

        if serv_mapper.exitStatus!=0:
            self.log.error("The tedge-mapper service is not running")
            self.abort(FAILED)

    def validate(self):
        #self.assertGrep("tedge_sub.out", "amessage", contains=True)
        #self.assertGrep("tedge_sub.out", "the message", contains=True)
        pass