import pysys
from pysys.basetest import BaseTest
from pysys.constants import *

import time

"""
Validate command line options config: get set unset

Given a running system
When we call tedge config list
Then then we get exit code 0

When we get and store the keys for later
When we unset all keys
Then we check if they are unset

When we override all keys with a string
Then we check if the key holds the string

When we set them again to the old value
Then we verify that the keys have the same value that was stored in the beginning

When we set device.id
Then we get a non zero exit code (not allowed to set)
When we unset device.id
Then we get a non zero exit code (not allowed to unset)


Note: Setting the device id is only allowed with tedge cert create.
Note: This is probably a bit complex for a test
Note: When this test is aborted the configuration might be invalid
"""

configdict = {
    "device.key.path": "",
    "device.cert.path": "",
    "c8y.url": "",
    "c8y.root.cert.path": "",
    "az.url": "",
    "az.root.cert.path": "",
}

DEFAULTKEYPATH = "/etc/tedge/device-certs/tedge-private-key.pem"
DEFAULTCERTPATH = "/etc/tedge/device-certs/tedge-certificate.pem"
DEFAULTROOTCERTPATH = "/etc/ssl/certs"


class PySysTest(BaseTest):
    def get_config_key(self, key):
        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "get", key],
            stdouterr="tedge_get_config_key",
            expectedExitStatus="==0",
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
            arguments=[self.tedge, "config", "unset", key],
            stdouterr="tedge_unset_config_key",
            expectedExitStatus="==0",
        )
        with open(proc.stdout) as data:
            value = data.read()
        return value

    def set_config_key(self, key, value):

        if value == None:
            self.unset_config_key(key)
            return

        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", key, value],
            stdouterr="tedge_set_config_key",
            expectedExitStatus="==0",
        )

    def execute(self):
        self.tedge = "/usr/bin/tedge"
        self.sudo = "/usr/bin/sudo"

        # get the config list do not expect entries
        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "list"],
            stdouterr="tedge",
            expectedExitStatus="==0",
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
            self.log.debug(f"Key: {key} Value: {valueread}")
            # Some values have defaults that are set instead of nothing:
            if key == "device.key.path":
                self.assertThat(
                    "expect == valueread", expect=DEFAULTKEYPATH, valueread=valueread
                )
            elif key == "device.cert.path":
                self.assertThat(
                    "expect == valueread", expect=DEFAULTCERTPATH, valueread=valueread
                )
            elif key == "c8y.root.cert.path":
                self.assertThat(
                    "expect == valueread",
                    expect=DEFAULTROOTCERTPATH,
                    valueread=valueread,
                )
            elif key == "az.root.cert.path":
                self.assertThat(
                    "expect == valueread",
                    expect=DEFAULTROOTCERTPATH,
                    valueread=valueread,
                )
            else:
                self.assertThat("expect == valueread", expect=None, valueread=valueread)

        # override all keys with a string
        for key in configdict.keys():
            expect = "failfailfail"
            self.set_config_key(key, expect)
            vread = self.get_config_key(key)
            self.assertThat("value == vread", value=expect, vread=vread)

        # set them again to the old value
        for key in configdict.keys():
            self.set_config_key(key, configdict[key])

        # print the keys for reference
        for key in configdict.keys():
            self.log.info(configdict[key])

        # verify that the keys have the same value that was stored in the beginning
        for key in configdict.keys():
            valueold = configdict[key]
            valuenew = self.get_config_key(key)
            self.assertThat(
                "valueold == valuenew", valueold=valueold, valuenew=valuenew
            )

        # special case: device.id unset is not allowed
        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "unset", "device.id"],
            stdouterr="tedge_unset_device_id",
            expectedExitStatus="!=0",
        )

        # special case: device.id unset is not allowed
        proc = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "config", "set", "device.id", "anotherid"],
            stdouterr="tedge_set_device_id",
            expectedExitStatus="!=0",
        )

    def validate(self):
        self.addOutcome(PASSED)
