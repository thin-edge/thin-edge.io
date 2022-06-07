import base64
import json
import re
import requests


class Cumulocity(object):
    """Class to retrieve information about Cumulocity.
    TODO : Review if we download enough data -> pageSize
    TODO : Documentation and test for all of these functions
    TODO : Extract as separate package
    TODO : Make this more bulletproof
    """

    c8y_url = ""
    tenant_id = ""
    username = ""
    password = ""
    auth = ""
    timeout_req = ""

    def __init__(self, c8y_url, tenant_id, username, password, log):
        self.c8y_url = c8y_url
        self.tenant_id = tenant_id
        self.username = username
        self.password = password
        self.timeout_req = 60  # seconds, got timeout with 60s
        self.log = log

        self.auth = ("%s/%s" % (self.tenant_id, self.username), self.password)

    def request(self, method, url_path, **kwargs) -> requests.Response:

        return requests.request(
            method, self.c8y_url + url_path, auth=self.auth, **kwargs
        )

    def get_all_devices(self) -> requests.Response:
        params = {"fragmentType": "c8y_IsDevice"}
        res = requests.get(
            url=self.c8y_url + "/inventory/managedObjects",
            params=params,
            auth=self.auth,
        )

        return self.to_json_response(res)

    def to_json_response(self, res: requests.Response):
        if res.status_code != 200:
            raise Exception(
                "Received invalid response with exit code: {}, reason: {}".format(
                    res.status_code, res.reason
                )
            )
        return json.loads(res.text)

    def get_all_devices_by_type(self, type: str) -> requests.Response:
        params = {
            "fragmentType": "c8y_IsDevice",
            "type": type,
            "pageSize": 100,
        }
        res = requests.get(
            url=self.c8y_url + "/inventory/managedObjects",
            params=params,
            auth=self.auth,
        )
        return self.to_json_response(res)

    def get_all_thin_edge_devices(self) -> requests.Response:
        return self.get_all_devices_by_type("thin-edge.io")

    def get_thin_edge_device_by_name(self, device_id: str):
        """
        TODO: Update : What is returned here ? Its the json data structure from C8y -
        Do we call this device fragment ?
        Hint: Device_id is the name of the device
        """

        # Hint this will fail, when the device does not have the type set "thin-edge.io" in C8y
        json_response = self.get_all_devices_by_type("thin-edge.io")

        for device in json_response["managedObjects"]:
            if device_id in device["name"]:
                return device
        return None

    def get_header(self):
        auth = bytes(f"{self.tenant_id}/{self.username}:{self.password}", "utf-8")
        header = {
            b"Authorization": b"Basic " + base64.b64encode(auth),
            b"content-type": b"application/json",
            b"Accept": b"application/json",
        }
        return header

    def trigger_log_request(self, log_file_request_payload, device_id):
        self.device_fragment = self.get_thin_edge_device_by_name(device_id)      
        url = f"{self.c8y_url}/devicecontrol/operations"
        log_file_request_payload = {
            "deviceId": self.device_fragment["id"],
            "description": "Log file request",
            "c8y_LogfileRequest": log_file_request_payload,
        }
 
        req = requests.post(
            url,
            json=log_file_request_payload,
            headers=self.get_header(),
            timeout=self.timeout_req,
        )

        jresponse = json.loads(req.text)

        operation_id = jresponse.get("id")

        if not operation_id:
            raise SystemError("field id is missing in response")

        return operation_id

    def retrieve_log_file(self, operation_id):
        """Check if log received"""

        url = f"{self.c8y_url}/devicecontrol/operations/{operation_id}"
        req = requests.get(url, headers=self.get_header(), timeout=self.timeout_req)

        req.raise_for_status()

        jresponse = json.loads(req.text)
        ret = ""

        log_response = jresponse.get("c8y_LogfileRequest")
        # check if the response contains the logfile
        log_file = log_response.get("file")
        self.log.info("log response %s", log_file)

        if log_file != None:
            ret = log_file
        return ret

    def get_child_device_of_thin_edge_device_by_name(
        self, thin_edge_device_id: str, child_device_id: str
    ):
        self.device_fragment = self.get_thin_edge_device_by_name(thin_edge_device_id)
        internal_id = self.device_fragment["id"]
        child_devices = self.to_json_response(
            requests.get(
                url="{}/inventory/managedObjects/{}/childDevices".format(
                    self.c8y_url, internal_id
                ),
                auth=self.auth,
            )
        )
        for child_device in child_devices["references"]:
            if child_device_id in child_device["managedObject"]["name"]:
                if child_device["managedObject"] == None:
                    print("Oh no it is None")
                    print(f"Cannot find {child_device_id}")
                return child_device["managedObject"]

        return None

    def get_last_measurements_from_device(self, device_internal_id: str):
        return self.get_last_n_measurements_from_device(
            device_internal_id=device_internal_id, target_size=1
        )[0]

    def get_last_n_measurements_from_device(
        self, device_internal_id: int, target_size: int
    ):
        params = {
            "source": device_internal_id,
            "pageSize": target_size,
            "dateFrom": "1970-01-01",
            "revert": True,
        }
        res = requests.get(
            url=self.c8y_url + "/measurement/measurements",
            params=params,
            auth=self.auth,
        )
        measurements_json = self.to_json_response(res)
        return measurements_json["measurements"]

    def get_last_n_alarms_from_device(
        self,
        device_internal_id: int,
        target_size=2000,
        status="ACTIVE",
        date_from="1970-01-01",
    ):
        params = {
            "source": device_internal_id,
            "pageSize": target_size,
            "dateFrom": "1970-01-01",
            "status": status,
        }
        res = requests.get(
            url=self.c8y_url + "/alarm/alarms", params=params, auth=self.auth
        )
        measurements_json = self.to_json_response(res)
        return measurements_json["alarms"]

    def get_last_alarm_from_device(self, device_internal_id: int):
        return self.get_last_n_alarms_from_device(
            device_internal_id=device_internal_id, target_size=1
        )[0]

    def clear_all_alarms_from_device(self, device_internal_id: int):
        params = {
            "source": device_internal_id,
        }
        payload = {"status": "CLEARED"}
        res = requests.put(
            url=self.c8y_url + "/alarm/alarms",
            params=params,
            json=payload,
            auth=self.auth,
        )
        res.raise_for_status()

    def delete_managed_object_by_internal_id(self, internal_id: str):
        res = requests.delete(
            url="{}/inventory/managedObjects/{}".format(self.c8y_url, internal_id),
            auth=self.auth,
        )
        if res.status_code != 204 or res.status_code != 404:
            res.raise_for_status()
