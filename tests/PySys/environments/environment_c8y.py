import json
import re
from pysys.constants import FAILED
import requests
from pysys.basetest import BaseTest

"""
Environment to manage automated connect and disconnect to c8y

Tests that derive from class EnvironmentC8y use automated connect and
disconnect to Cumulocity. Additional checks are made for the status of
service mosquitto and service tedge-mapper.
"""


class Cumulocity(object):
    """Class to retrieve information about Cumulocity.
    TODO : Review if we download enough data -> pageSize
    """

    c8y_url = ""
    tenant_id = ""
    username = ""
    password = ""
    auth = ""

    def __init__(self, c8y_url, tenant_id, username, password):
        self.c8y_url = c8y_url
        self.tenant_id = tenant_id
        self.username = username
        self.password = password

        self.auth = ('%s/%s' % (self.tenant_id, self.username), self.password)

    def request(self, method, url_path, **kwargs) -> requests.Response:
        return requests.request(method, self.c8y_url + url_path, auth=self.auth, **kwargs)

    def get_all_devices(self) -> requests.Response:
        params = {
            "fragmentType": "c8y_IsDevice"
        }
        res = requests.get(
            url=self.c8y_url + "/inventory/managedObjects", params=params, auth=self.auth)

        return self.to_json_response(res)

    def to_json_response(self, res: requests.Response):
        if res.status_code != 200:
            raise Exception(
                "Received invalid response with exit code: {}, reason: {}".format(res.status_code, res.reason))
        return json.loads(res.text)

    def get_all_devices_by_type(self, type: str) -> requests.Response:
        params = {
            "fragmentType": "c8y_IsDevice",
            "type": type,
            "pageSize": 100,
        }
        res = requests.get(
            url=self.c8y_url + "/inventory/managedObjects", params=params, auth=self.auth)
        return self.to_json_response(res)

    def get_all_thin_edge_devices(self) -> requests.Response:
        return self.get_all_devices_by_type("thin-edge.io")

    def get_thin_edge_device_by_name(self, device_id: str):
        json_response = self.get_all_devices_by_type("thin-edge.io")
        for device in json_response['managedObjects']:
            if device_id in device['name']:
                return device
        return None

    def get_child_device_of_thin_edge_device_by_name(self, thin_edge_device_id: str, child_device_id: str):
        self.device_fragment = self.get_thin_edge_device_by_name(
            thin_edge_device_id)
        internal_id = self.device_fragment['id']
        child_devices = self.to_json_response(requests.get(
            url="{}/inventory/managedObjects/{}/childDevices".format(self.c8y_url, internal_id), auth=self.auth))
        for child_device in child_devices['references']:
            if child_device_id in child_device['managedObject']['name']:
                return child_device['managedObject']

        return None

    def get_last_measurements_from_device(self, device_internal_id: str):
        params = {
            "source": device_internal_id,
            "pageSize": 1,
            "revert": True
        }
        res = requests.get(
            url=self.c8y_url + "/measurement/measurements", params=params, auth=self.auth)
        measurements_json = self.to_json_response(res)
        return measurements_json['measurements'][0]

    def delete_managed_object_by_internal_id(self, internal_id: str):
        res = requests.delete(
            url="{}/inventory/managedObjects/{}".format(self.c8y_url, internal_id), auth=self.auth)
        if res.status_code != 204 or res.status_code != 404:
            res.raise_for_status()


class EnvironmentC8y(BaseTest):
    cumulocity: Cumulocity

    def setup(self):
        self.log.debug("EnvironmentC8y Setup")

        if self.project.c8yurl == "":
            self.abort(
                FAILED, "Cumulocity tenant URL is not set. Set with the env variable C8YURL")
        if self.project.tenant == "":
            self.abort(
                FAILED, "Cumulocity tenant ID is not set. Set with the env variable C8YTENANT")
        if self.project.username == "":
            self.abort(
                FAILED, "Cumulocity tenant username is not set. Set with the env variable C8YUSERNAME")
        if self.project.c8ypass == "":
            self.abort(
                FAILED, "Cumulocity tenant password is not set. Set with the env variable C8YPASS")
        if self.project.deviceid == "":
            self.abort(
                FAILED, "Device ID is not set. Set with the env variable C8YDEVICEID")

        self.tedge = "/usr/bin/tedge"
        self.tedge_mapper_c8y = "tedge-mapper-c8y"
        self.sudo = "/usr/bin/sudo"
        self.systemctl = "/usr/bin/systemctl"
        self.log.info("EnvironmentC8y Setup")
        self.addCleanupFunction(self.myenvcleanup)

        # Check if tedge-mapper is in disabled state
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper1",
            expectedExitStatus="==3",  # 3: disabled
        )

        # Connect the bridge
        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y"],
            stdouterr="tedge_connect",
        )

        # Test the bridge connection
        connect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "connect", "c8y", "--test"],
            stdouterr="tedge_connect_test",
        )

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", "mosquitto"],
            stdouterr="serv_mosq2",
        )

        # Check if tedge-mapper is active again
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper3",
        )

        self.cumulocity = Cumulocity(
            self.project.c8yurl, self.project.tenant, self.project.username, self.project.c8ypass)

    def execute(self):
        self.log.debug("EnvironmentC8y Execute")

    def validate(self):
        self.log.debug("EnvironmentC8y Validate")

        # Check if mosquitto is running well
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", "mosquitto"],
            stdouterr="serv_mosq",
        )

        # Check if tedge-mapper is active
        serv_mapper = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper4",
        )

    def myenvcleanup(self):
        self.log.debug("EnvironmentC8y Cleanup")

        # Disconnect Bridge
        disconnect = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "disconnect", "c8y"],
            stdouterr="tedge_disconnect",
        )

        # Check if tedge-mapper is disabled
        serv_mosq = self.startProcess(
            command=self.systemctl,
            arguments=["status", self.tedge_mapper_c8y],
            stdouterr="serv_mapper5",
            expectedExitStatus="==3",
        )
