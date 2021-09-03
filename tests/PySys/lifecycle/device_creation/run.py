import json
from environment_c8y import EnvironmentC8y
from pysys.basetest import BaseTest


class DeviceCreationTest(EnvironmentC8y):

    def execute(self):
        super().execute()
        self.device_fragment = self.cumulocity.getThinEdgeDeviceByName("albin")

    def validate(self):
        super().validate()
        self.assertTrue(self.device_fragment != None)
