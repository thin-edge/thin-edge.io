import pysys
from pysys.basetest import BaseTest
from pysys.constants import *

import time

"""
Validate command line option config list

Note: Setting the device id is only allowed with tedge cert create.
"""

configdict = {"device.key.path":"", "device.cert.path":"",
        "c8y.url":"", "c8y.root.cert.path":"",
        "azure.url":"", "azure.root.cert.path":""}

DEFAULTKEYPATH="/etc/tedge/device-certs/tedge-private-key.pem"
DEFAULTCERTPATH="/etc/tedge/device-certs/tedge-certificate.pem"

class PySysTest(BaseTest):

    def get_config_key(self, key):
        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge,"config", "get", key],
            stdouterr="tedge_get_config_key",
            expectedExitStatus='==0',
        )
        with open(proc.stdout) as data:
            value = data.read().strip()

        # Do not set when this is in stdout:
        # "The provided config key: 'c8y.url' is not set"
        if "is not set" in value:
            return None
        else:
            return value

    def unset_config_key(self, key):
        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge,"config", "unset", key],
            stdouterr="tedge_unset_config_key",
            expectedExitStatus='==0',
        )
        with open(proc.stdout) as data:
            value = data.read()
        return value

    def set_config_key(self, key, value):

        if value == None:
            return

        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge,"config", "set", key, value],
            stdouterr="tedge_set_config_key",
            expectedExitStatus='==0',
        )
        with open(proc.stdout) as data:
            value = data.read()
        return value

    def execute(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # get the config list do not expect entries
        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "list"],
            stdouterr="tedge",
            expectedExitStatus='==0',
        )

        # get and store the keys for later
        for key in configdict.keys():
            value = self.get_config_key(key)
            configdict[key] = value

        # print the keys for reference
        for key in configdict.keys():
            self.log.info(configdict[key])

        # unset all keys
        for key in configdict.keys():
            value = self.unset_config_key(key)

        # check if they are unset
        for key in configdict.keys():
            valueread = self.get_config_key(key)

            # Some values have defaults that are set instead of nothing:
            if key == "device.key.path":
                self.assertThat("expect == valueread", expect=DEFAULTKEYPATH,
                    valueread=valueread)
            elif key== "device.cert.path":
                self.assertThat("expect == valueread", expect=DEFAULTCERTPATH,
                    valueread=valueread)
            else:
                self.assertThat("expect == valueread", expect=None, valueread=valueread)

        # set them again to the old value
        for key in configdict.keys():
            value = self.set_config_key(key, configdict[key])

        # print the keys for reference
        for key in configdict.keys():
            self.log.info(configdict[key])

        for key in configdict.keys():
            valueold = configdict[key]
            valuenew = self.get_config_key(key)
            self.assertThat("valueold == valuenew", valueold=valueold, valuenew=valuenew)


    def validate(self):
        self.addOutcome(PASSED)

