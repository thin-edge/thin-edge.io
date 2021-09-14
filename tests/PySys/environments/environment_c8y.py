import json
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


class EnvironmentC8y(BaseTest):
    cumulocity: Cumulocity

    def setup(self):
        self.log.debug("EnvironmentC8y Setup")

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

        if self.project.c8yurl == "":
            self.abort(FAILED, "Cumulocity tenant URL is not set")
        if self.project.tenant == "":
            self.abort(FAILED, "Cumulocity tenant ID is not set")
        if self.project.username == "":
            self.abort(FAILED, "Cumulocity tenant username is not set")
        if self.project.c8ypass == "":
            self.abort(FAILED, "Cumulocity tenant password is not set")

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
