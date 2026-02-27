"""ThinEdgeIO Library for Robot Framework

It enables the creation of devices which can be used in tests.
It currently support the creation of Docker devices only
"""

# pylint: disable=invalid-name

import inspect
import logging
import json
from typing import Any, Union, List, Dict, Optional, Tuple
import time
from dataclasses import dataclass
from datetime import datetime
import re
import base64
import os
import shutil
import subprocess
from pathlib import Path

from paho.mqtt import matcher
from robot.api.deco import keyword, library
from robot.libraries.BuiltIn import BuiltIn
from DeviceLibrary import DeviceLibrary, DeviceAdapter
from DeviceLibrary.DeviceLibrary import timestamp
from Cumulocity import Cumulocity, retry

devices_lib = DeviceLibrary()
c8y_lib = Cumulocity()

logging.basicConfig(
    level=logging.DEBUG, format="%(asctime)s %(module)s -%(levelname)s- %(message)s"
)
log = logging.getLogger(__name__)

__version__ = "0.0.1"
__author__ = "Reuben Miller"
C8Y_TOKEN_TOPIC = "c8y/s/dat"


@dataclass
class Certificate:
    """Certificate details"""

    issuer: str = ""
    subject: str = ""
    thumbprint: str = ""

    @classmethod
    def from_dict(cls, env):
        """Parse certificate information from a dictionary"""
        return cls(
            **{k: v for k, v in env.items() if k in inspect.signature(cls).parameters}
        )

    @property
    def is_self_signed(self) -> bool:
        """check if the certificate is self-signed (e.g. issuer == subject)"""
        if self.issuer is None or self.subject is None:
            return False
        return bool(self.issuer and self.issuer == self.subject)


class MQTTMessage:
    """MQTT Message"""

    timestamp: float
    topic: str
    qos: str
    retain: int
    payloadlen: int
    payload: str

    @property
    def date(self) -> datetime:
        """Get the message timestamp as a datetime object"""
        return datetime.fromtimestamp(self.timestamp)

    # {"tst":"2022-12-27T16:55:44.923776Z+0000","topic":"c8y/s/us","qos":0,"retain":0,"payloadlen":99,"payload":"119,c8y-bridge.conf,c8y-configuration-plugin,example,mosquitto.conf,tedge-mosquitto.conf,tedge.toml"}


def strip_scheme(url: Optional[str]) -> Optional[str]:
    """Strip the scheme from a URL"""
    if isinstance(url, str):
        # strip any scheme if present
        return url.strip().split("://", 1)[-1]
    return url


@library(scope="SUITE", auto_keywords=False)
class ThinEdgeIO(DeviceLibrary):
    """ThinEdgeIO Library"""

    def __init__(
        self,
        image: str = DeviceLibrary.DEFAULT_IMAGE,
        adapter: Optional[str] = None,
        bootstrap_script: str = DeviceLibrary.DEFAULT_BOOTSTRAP_SCRIPT,
        **kwargs,
    ):
        super().__init__(
            image=image, adapter=adapter, bootstrap_script=bootstrap_script, **kwargs
        )

        # track self-signed devices certificates for cleanup after the suite has finished
        self._certificates: Dict[str, str] = {}

        # Configure retries
        retry.configure_retry_on_members(self, "^_assert_")

        # Cumulocity configuration cache
        self._c8y_config = {}

    @property
    def c8y_config(self) -> Dict[str, Any]:
        """Get the Cumulocity configuration"""
        if not self._c8y_config:
            self._c8y_config = BuiltIn().get_variable_value(r"&{C8Y_CONFIG}", {}) or {}
        return self._c8y_config

    @property
    def c8y_host(self) -> str:
        """Get the Cumulocity domain/host"""
        return c8y_lib.get_domain()

    @property
    def c8y_mqtt(self) -> Optional[str]:
        """Get the Cumulocity MQTT broker URL"""
        return strip_scheme(self.c8y_config.get("mqtt"))

    def end_suite(self, _data: Any, result: Any):
        """End suite hook which is called by Robot Framework
        when the test suite has finished

        Args:
            _data (Any): Test data
            result (Any): Test details
        """
        log.info("Suite %s (%s) ending", result.name, result.message)

        log.info(
            "Removing the following self-signed certificates (thumbprints): %s",
            self._certificates,
        )
        for thumbprint, device_sn in self._certificates.items():
            try:
                self.remove_certificate(thumbprint)
                c8y_lib.device_mgmt.inventory.delete_device_and_user(
                    device_sn, "c8y_Serial"
                )
            except Exception as ex:
                log.warning("Could not cleanup certificate/device. %s", ex)

        # remove device management objects and related users
        # Note: this needs to run in addition to the certificate cleanup
        # for device that don't use self-signed certificate, and delete_device_and_user
        # does a no-op if the managed object and/or user does not exist, so it is safe
        # to run multiple times
        for device in self.devices.values():
            try:
                device_sn = device.get_id()
                # Note: this is a no-op if the device or user does not exist
                c8y_lib.device_mgmt.inventory.delete_device_and_user(
                    device_sn, "c8y_Serial"
                )
            except Exception as ex:
                log.warning("Could not cleanup device. %s", ex)

        super().end_suite(_data, result)

    def end_test(self, _data: Any, result: Any):
        """End test hook which is called by Robot Framework
        when the test has ended

        Args:
            _data (Any): Test data
            result (Any): Test details
        """
        log.info("Listener: detected end of test")
        if not result.passed:
            log.info("Test '%s' failed: %s", result.name, result.message)

        # store self-signed certificates before anything else is done with the devices
        # record each self-signed certificate within the current set of devices
        # as the certificates can change within tests which would result in the suite
        # teardown not knowing about any intermediate artifacts
        for device in self.devices.values():
            try:
                if isinstance(device, DeviceAdapter) and device.should_cleanup:
                    certificate = self.get_certificate_details(device)
                    if certificate.is_self_signed and certificate.thumbprint:
                        self._certificates[certificate.thumbprint] = device.get_id()
            except Exception as ex:
                log.warning("Could not cleanup certificate/device. %s", ex)

        super().end_test(_data, result)

    @keyword("Register Certificate For Cleanup")
    def register_certificate(
        self,
        cloud_profile: Optional[str] = None,
        common_name: Optional[str] = None,
        device_name: Optional[str] = None,
    ):
        """Register a self-signed certificate for deletion after the test suite
        has finished.

        Args:
            device_name (Optional[str], optional): device name. Defaults to current device.
            cloud_profile (Optional[str], optional): Cloud profile name. Defaults to None.

        Raises:
            ValueError: No device context given
        """
        device = self.get_device(device_name)

        certificate = self.get_certificate_details(device, cloud_profile=cloud_profile)
        if certificate.is_self_signed and certificate.thumbprint:
            self._certificates[certificate.thumbprint] = common_name or device.get_id()

    @keyword("Register Device With Cumulocity CA")
    def register_device_with_cumulocity_ca(
        self,
        external_id: str,
        external_type: str = "c8y_Serial",
        name: Optional[str] = None,
        device_type: str = "thin-edge.io",
        device_name: Optional[str] = None,
        csr_path: Optional[str] = None,
        http: Optional[str] = None,
        mqtt: Optional[str] = None,
        **kwargs,
    ):
        """Register a device with Cumulocity using the Cumulocity Certificate Authority feature

        Examples:

        | Register Device With Cumulocity CA | external_id=tedge001 |
        | Register Device With Cumulocity CA | external_id=tedge001 | name=Custom Tedge 0001 |
        """
        credentials = c8y_lib.device_mgmt.registration.bulk_register_with_ca(
            external_id=external_id,
            external_type=external_type,
            name=name,
            device_type=device_type,
        )

        self.set_cumulocity_urls(http=http, mqtt=mqtt, device_name=device_name)

        cmd = f"tedge cert download c8y --device-id '{external_id}' --one-time-password '{credentials.one_time_password}' --retry-every 5s --max-timeout 60s"
        if csr_path:
            cmd += f" --csr-path '{csr_path}'"

        self.execute_command(
            cmd,
            device_name=device_name,
        )

    @keyword("Set Cumulocity URLs")
    def set_cumulocity_urls(
        self,
        http: Optional[str] = None,
        mqtt: Optional[str] = None,
        profile: Optional[str] = None,
        device_name: Optional[str] = None,
    ):
        """Set the Cumulocity URLs. If an mqtt url is defined then the individual urls will be set
        otherwise only the single c8y.url will be set.

        Args:
            http (Optional[str]): The HTTP URL to set. If set to None, then the default host will be used.
            mqtt (Optional[str]): The MQTT URL to set. If set to None, then the default mqtt broker will be used (based on the host).
            device_name (Optional[str]): The device name to set.
            profile (Optional[str]): Cloud profile. Defaults to None.
        """

        if mqtt is None:
            mqtt = self.c8y_mqtt

        if http is None:
            http = self.c8y_host

        def _append_command(cmd):
            if profile:
                commands.append(f"{cmd} --profile {profile}")
            else:
                commands.append(cmd)

        commands = []
        if not mqtt:
            _append_command(f"tedge config set c8y.url '{http}'")
        else:
            _append_command(f"tedge config set c8y.mqtt '{mqtt}'")
            _append_command(f"tedge config set c8y.http '{http}'")

        cmd = " && ".join(commands)
        self.execute_command(
            cmd,
            device_name=device_name,
        )

    @keyword("Register Device With Self-Signed Certificate")
    def register_device_with_self_signed_certificate(
        self,
        external_id: str,
        device_name: Optional[str] = None,
        auto_registration_enabled: bool = True,
        http: Optional[str] = None,
        mqtt: Optional[str] = None,
        **kwargs,
    ):
        """Register a device with Cumulocity using a self-signed certificate. The self-signed
        certificate is updated

        Examples:

        | Register Device With Self-Signed Certificate | external_id=tedge001 |
        """
        self.set_cumulocity_urls(http=http, mqtt=mqtt, device_name=device_name)
        self.execute_command(f"tedge cert create --device-id '{external_id}'")
        pem_contents = self.execute_command(
            'cat "$(tedge config get device.cert_path)"'
        )
        c8y_lib.upload_trusted_certificate(
            name=external_id,
            pem_cert=pem_contents,
            auto_registration_enabled=auto_registration_enabled,
            ignore_duplicate=True,
        )
        self.register_certificate(common_name=external_id, device_name=device_name)

    @keyword("Get Debian Architecture")
    def get_debian_architecture(self):
        """Get the debian architecture"""
        return self.execute_command(
            "dpkg --print-architecture", strip=True, stdout=True, stderr=False
        )

    @keyword("Get Suite Logs")
    def get_suite_logs(self, name: Optional[str] = None, show=True):
        """Get device logs from the start of the suite.

        See 'Get Logs' Keyword for the full details.

        Args:
            name (str, optional): Device name to get logs for. Defaults to None.
            show (boolean, optional): Show/Display the log entries

        Returns:
            List[str]: List of log lines

        *Example:*
        | `Suite Teardown` | Get Suite Logs | name=${PARENT_SN} |
        """
        self.get_logs(name=name, date_from=self.suite_start_time, show=show)

    @keyword("Get Logs")
    def get_logs(
        self,
        name: Optional[str] = None,
        date_from: Optional[timestamp.Timestamp] = None,
        show=True,
    ) -> List[str]:
        """Get device logs (override base class method to add additional debug info)

        Note: the date_from only applies to the systemd logs (not file based logs). This is
        a technical limitation as there is no easy way to do time based filtering on the content.

        Args:
            name (str, optional): Device name to get logs for. Defaults to None.
            date_from (Union[datetime, float]: Only include logs starting from a specific datetime
                Accepts either datetime object, or a float (linux timestamp) in seconds.
            show (boolean, optional): Show/Display the log entries

        Returns:
            List[str]: List of log lines

        *Example:*
        | `Test Teardown` | | | | | Get Logs | | | | | name=${PARENT_SN} |
        """
        device = self.current
        if name:
            if name in self.devices:
                device = self.devices.get(name)

        if not device:
            log.info("Device has not been setup, so no logs to collect")
            return []

        device_sn = name or device.get_id()
        try:
            managed_object = c8y_lib.c8y.identity.get_object(device_sn, "c8y_Serial")
            log.info(
                "Managed Object\n%s", json.dumps(managed_object.to_json(), indent=2)
            )
            self.log_operations(managed_object.id)
        except KeyError:
            log.info(
                "Skip getting managed object as it has not been registered in Cumulocity"
            )
        except Exception as ex:  # pylint: disable=broad-except
            # Only log info as not all tests require creating an object in Cumulocity
            log.warning("Failed to get device managed object. %s", ex)

        # Log mqtt messages separately so it is easier to read/debug
        try:
            self.log_mqtt_messages("#", date_from)
        except Exception as ex:
            log.warning("Failed to retrieve mqtt logs. %s", ex, exc_info=True)

        try:
            # Get agent log files (if they exist)
            log.info("tedge agent logs: /var/log/tedge/agent/*")
            device.execute_command(
                "tail -n +1 /var/log/tedge/agent/* 2>/dev/null || true",
                shell=True,
            )
        except Exception as ex:
            log.warning("Failed to retrieve logs. %s", ex, exc_info=True)

        try:
            # entity_store.jsonl log (if they exist)
            log.info("entity_store.jsonl log:")
            device.execute_command(
                "head -n-1 /etc/tedge/.tedge-mapper-c8y/entity_store.jsonl /etc/tedge/.agent/entity_store.jsonl 2>/dev/null || true",
                shell=True,
            )
        except Exception as ex:
            log.warning("Failed to retrieve logs. %s", ex, exc_info=True)

        log_output = super().get_logs(device.get_id(), date_from=date_from, show=False)
        if show:
            hide_sensitive = self._hide_sensitive_factory()
            for line in log_output:
                print(hide_sensitive(line))

        return log_output

    def _hide_sensitive_factory(self):
        # This is fragile and should be improved upon once a more suitable/robust method of logging and querying
        # the mqtt messages is found.
        token_replace_pattern = re.compile(r"\{.+$")

        def _hide(line: str) -> str:
            if C8Y_TOKEN_TOPIC in line and "71," in line:
                line_sensitive = token_replace_pattern.sub(
                    f"(redacted log entry): Received token: topic={C8Y_TOKEN_TOPIC}, message=71,<redacted>",
                    line,
                )
                return line_sensitive
            return line

        return _hide

    def log_operations(self, mo_id: str, status: Optional[str] = None):
        """Log operations to help with debugging

        Args:
            mo_id (str): Managed object id
            status (str, optional): Operation status. Defaults to None (which means all statuses).
        """
        filter_args = {}
        if status:
            filter_args["status"] = status
        if self.test_start_time:
            filter_args["after"] = self.test_start_time
        operations = c8y_lib.c8y.operations.get_all(
            device_id=mo_id,
            **filter_args,
        )

        if operations:
            log.info("%s operations", status or "ALL")
            for i, operation in enumerate(operations):
                # Only treat operations which did not finish
                # as errors (as FAILED might be intended in a few test cases)
                log_method = (
                    log.info
                    if operation.status
                    in (operation.Status.SUCCESSFUL, operation.Status.FAILED)
                    else log.warning
                )
                log_method(
                    "Operation %d: (status=%s)\n%s",
                    i + 1,
                    operation.status,
                    json.dumps(operation.to_json(), indent=2),
                )
        else:
            log.info("No operations found")

    def get_certificate_details(
        self,
        device: Optional[DeviceAdapter] = None,
        cloud_profile: Optional[str] = None,
    ) -> Certificate:
        """Get the details about the device's certificate

        Args:
            device (DeviceAdapter, optional): Device. Defaults to the current device.
            cloud_profile (Optional[str], optional): Optional cloud profile name. Defaults to None.

        Returns:
            Certificate: Information about the current certificate
        """
        certificate = Certificate()
        if device is None:
            device = self.current

        if not device:
            log.info("No certificate to remove as the device as not been set")
            return certificate

        # Parse the certificate details via tedge cert show
        lines = []
        try:
            command = "tedge cert show c8y"
            if cloud_profile:
                command += f" --profile {cloud_profile}"
            lines = self.execute_command(command, ignore_exit_code=True).splitlines()

        except Exception as ex:
            # Ignore any errors
            log.info("Could not read certificate information. %s", ex)
            return certificate

        # Prase output and decode the certificate information
        # Use simple parser to avoid having to decode the certificate
        fields = {}
        for line in lines:
            key, _, value = line.partition(":")
            if key and value:
                fields[key.lower().strip()] = value.strip()

        certificate = Certificate.from_dict(fields)
        return certificate

    def remove_certificate(self, thumbprint: str):
        """Remove trusted certificate

        Args:
            thumbprint (str): Certificate thumbprint/fingerprint
        """
        try:
            c8y_lib.trusted_certificate_delete(thumbprint.lower())
        except Exception as ex:
            log.warning("Could not remove device certificate. error=%s", ex)

    def remove_device(self, device: Optional[DeviceAdapter] = None):
        """Remove device from the cloud"""
        if device is None:
            device = self.current

        if not device:
            log.info("No device to remove as device context is not set")
            return

        try:
            device_sn = device.get_id()
            if not device_sn:
                log.info(
                    "Device serial number is empty, so nothing to delete from Cumulocity"
                )
                return
            c8y_lib.device_mgmt.inventory.delete_device_and_user(
                device_sn, "c8y_Serial"
            )
        except KeyError:
            log.info("Device does not exist in cloud, nothing to delete")
        except Exception as ex:
            log.warning("Could not remove device. error=%s", ex)

    @keyword("Download From GitHub")
    def download_from_github(self, *run_id: str, arch: str = "aarch64"):
        """Download artifacts from a GitHub Run

        Args:
            *run_id (str): Run ids of the artifacts to download
            arch (str, optional): CPU Architecture to download for. Defaults to aarch64
        """

        # pylint: disable=line-too-long
        self.execute_command("""
            type -p curl >/dev/null || sudo apt install curl -y
            curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \\
            && sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \\
            && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null \\
            && sudo apt-get update \\
            && sudo apt-get -y install gh
        """.lstrip())

        run_ids = []
        # Also support providing values via csv (e.g. when set from variables)
        for i_run_id in run_id:
            run_ids.extend(i_run_id.split(","))

        # Support mapping debian architecture names to the rust (e.g. arm64 -> aarch64)
        arch_mapping = {
            # TODO: Extend to include all supported types, e.g. amd64 etc. Check what to do about armv6l
            "armhf": "armv7",
            "arm64": "aarch64",
        }
        arch = arch_mapping.get(arch, arch)

        for i_run_id in run_ids:
            self.execute_command(f"""
                gh run download {i_run_id} -n debian-packages-{arch}-unknown-linux-gnu -R thin-edge/thin-edge.io
            """.strip())

    #
    # Tedge commands
    #
    @keyword("Set Tedge Configuration Using CLI")
    def tedge_update_settings(self, name: str, value: str) -> str:
        """Update tedge settings via CLI (`tedge config set`)

        Args:
            name (str): Setting name to update
            value (str): Value to be updated with

        *Example:*

        Update known setting
        | ${DEVICE_SN} | | | | | Setup |
        | `Set Tedge Configuration Using CLI` | | | | | device.type | | | | | mycustomtype |
        | ${OUTPUT}= | | | | | `Execute Command` | | | | | tedge config get device.type |
        | `Should Match` | | | | | ${OUTPUT} | | | | | mycustomtype\\n |

        Returns:
            str: Command output
        """
        return self.execute_command(
            f"tedge config set {name} {value}", stdout=True, stderr=False
        )

    @keyword("Connect Mapper")
    def tedge_connect(self, mapper: str = "c8y") -> str:
        """Tedge connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".

        Returns:
            str: Command output
        """
        return self.execute_command(
            f"tedge connect {mapper}", stdout=True, stderr=False
        )

    @keyword("Disconnect Mapper")
    def tedge_disconnect(self, mapper: str = "c8y") -> str:
        """Tedge connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".

        Returns:
            str: Command output
        """
        return self.execute_command(
            f"tedge disconnect {mapper}", stdout=True, stderr=False
        )

    @keyword("Disconnect Then Connect Mapper")
    def tedge_disconnect_connect(self, mapper: str = "c8y", sleep: float = 0.0):
        """Tedge disconnect the connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".
            sleep (float, optional): Time to wait in seconds before connecting. Defaults to 0.0.

        *Examples:*
        | `Disconnect Then Connect Mapper` | | | | | c8y |
        """
        self.tedge_disconnect(mapper)
        if sleep > 0:
            time.sleep(sleep)
        self.tedge_connect(mapper)

    #
    # MQTT
    # TODO:
    # Assert presence of a topic (with timeout)
    #
    def mqtt_match_messages(
        self,
        topic: str,
        message_pattern: Optional[str] = None,
        date_from: Optional[timestamp.Timestamp] = None,
        date_to: Optional[timestamp.Timestamp] = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
        """Match mqtt messages using different types of filters

        Args:
            topic (str): Filter by topic

        """
        cmd = "journalctl -u mqtt-logger.service -n 1000 --output=cat"

        if not date_from:
            date_from = self.test_start_time

        if date_from:
            cmd += f" --since '@{timestamp.parse_timestamp(date_from).timestamp()}'"

        if date_to:
            cmd += f" --until '@{timestamp.parse_timestamp(date_to).timestamp()}'"

        output = self.execute_command(cmd, log_output=False, stdout=True, stderr=False)

        messages = []
        message_pattern_re = None
        if message_pattern:
            message_pattern_re = re.compile(message_pattern, re.IGNORECASE)

        for line in output.splitlines():
            try:
                message = json.loads(line)
                if "message" in message:
                    if message_pattern_re is None or message_pattern_re.match(
                        message["message"]["payload"]
                    ):
                        messages.append(message)
            except Exception as ex:
                log.debug("ignoring non-json entry. %s", ex)

        mqtt_matcher = matcher.MQTTMatcher()
        mqtt_matcher[topic] = True

        matching = [
            item
            for item in messages
            if not topic
            or (topic and mqtt_topic_match(mqtt_matcher, item["message"]["topic"]))
        ]
        return matching

    #
    # Service Health Status
    #
    @keyword("Service Health Status Should Be Up")
    def assert_service_health_status_up(
        self, service: str, device: str = "main", **kwargs
    ) -> Dict[str, Any]:
        """Checks if the Service Health Status is up

        *Examples:*

        | `Service Health Status Should Be Up` | tedge-mapper-c8y |
        | `Service Health Status Should Be Up` | tedge-mapper-c8y | device=child01 |
        """
        return self._assert_health_status(service, status="up", device=device, **kwargs)

    @keyword("Service Health Status Should Be Down")
    def assert_service_health_status_down(
        self, service: str, device: str = "main", **kwargs
    ) -> Dict[str, Any]:
        """Checks if the Service Health Status is down

        *Examples:*

        | `Service Health Status Should Be Down` | tedge-mapper-c8y |
        | `Service Health Status Should Be Down` | tedge-mapper-c8y | device=child01 |
        """
        return self._assert_health_status(
            service, status="down", device=device, **kwargs
        )

    @keyword("Service Health Status Should Be Equal")
    def assert_service_health_status_equal(
        self, service: str, status: str, device: str = "main", **kwargs
    ) -> Dict[str, Any]:
        """Checks if the Service Health Status is equal to a given status

        *Examples:*

        | `Service Health Status Should Be Equal` | tedge-mapper-c8y | status=up |
        | `Service Health Status Should Be Equal` | tedge-mapper-c8y | status=up | device=child01 |
        """
        return self._assert_health_status(service, status=status, device=device)

    def _assert_health_status(
        self, service: str, status: str, device: str = "main", **kwargs
    ) -> Dict[str, Any]:
        # if mqtt.client.auth.ca_file or mqtt.client.auth.ca_dir is set, we pass setting
        # value to mosquitto_sub
        mqtt_config_options = self.execute_command(
            "tedge config list", stdout=True, stderr=False, ignore_exit_code=True
        )

        server_auth = ""
        if "mqtt.client.auth.ca_file" in mqtt_config_options:
            server_auth = "--cafile /etc/mosquitto/ca_certificates/ca.crt"

        client_auth = ""
        if "mqtt.client.auth.cert_file" in mqtt_config_options:
            client_auth = "--cert /setup/client.crt --key /setup/client.key"

        message = self.execute_command(
            f"mosquitto_sub -t 'te/device/{device}/service/{service}/status/health' --retained-only -C 1 -W 5 -h $(tedge config get mqtt.client.host) -p $(tedge config get mqtt.client.port) {server_auth} {client_auth}",
            stdout=True,
            stderr=False,
        )
        current_status = ""
        health = {}
        try:
            health = json.loads(message)
            current_status = health.get("status", "")
        except Exception as ex:
            log.debug(
                "Detected non json health message. json_error=%s, message=%s",
                ex,
                message,
            )
            # Convert mosquitto 1/0 to up/down
            status_mappings = {"1": "up", "0": "down"}
            current_status = status_mappings.get(message.strip(), "unknown")
        assert current_status == status
        return health

    @keyword("Setup")
    def setup(
        self,
        skip_bootstrap: Optional[bool] = None,
        bootstrap_args: Optional[str] = None,
        cleanup: Optional[bool] = None,
        adapter: Optional[str] = None,
        env_file: str = ".env",
        register: bool = True,
        register_using: str = "c8y-ca",
        connect: bool = True,
        **adaptor_config,
    ) -> str:
        """_summary_

        Args:
            skip_bootstrap (bool, optional): Don't run the bootstrap script. Defaults to None
            bootstrap_args (str, optional): Additional arguments to be passed to the bootstrap
                command. Defaults to None.
            cleanup (bool, optional): Should the cleanup be run or not. Defaults to None
            adapter (str, optional): Type of adapter to use, e.g. ssh, docker etc. Defaults to None
            **adaptor_config: Additional configuration that is passed to the adapter. It will override
                any existing settings.

            env_file (str, optional): dotenv file to pass to the adapter. Defaults to ".env".
            register (bool, optional): Register the device with Cumulocity. Defaults to True.
            register_using (str, optional): Registration method. Supported values: [c8y-ca, self-signed]. Defaults to "c8y-ca".
            connect (bool, optional): Connect the mapper to Cumulocity. Defaults to True.

        Raises:
            ValueError: Invalid 'register_using'

        Returns:
            str: Serial number
        """
        serial_sn = super().setup(
            skip_bootstrap=skip_bootstrap,
            bootstrap_args=bootstrap_args,
            cleanup=cleanup,
            adapter=adapter,
            env_file=env_file,
            **adaptor_config,
        )

        if not skip_bootstrap:
            if register:
                if register_using == "c8y-ca":
                    self.register_device_with_cumulocity_ca(external_id=serial_sn)
                elif register_using == "self-signed":
                    self.register_device_with_self_signed_certificate(
                        external_id=serial_sn
                    )
                else:
                    raise ValueError(
                        "Invalid 'register_using' value. Supported values: [c8y-ca, self-signed]"
                    )

                if connect:
                    self.tedge_connect("c8y")

        return serial_sn

    def _assert_mqtt_topic_messages(
        self,
        topic: str,
        date_from: Optional[timestamp.Timestamp] = None,
        date_to: Optional[timestamp.Timestamp] = None,
        minimum: int = 1,
        maximum: Optional[int] = None,
        message_pattern: Optional[str] = None,
        message_contains: Optional[str] = None,
        **kwargs,
    ) -> List[str]:
        # log.info("Checking mqtt messages for topic: %s", topic)
        if message_contains:
            message_pattern = r".*" + re.escape(message_contains) + r".*"

        items = self.mqtt_match_messages(
            topic=topic,
            date_from=date_from,
            date_to=date_to,
            message_pattern=message_pattern,
            **kwargs,
        )

        messages = [
            bytes.fromhex(item["payload_hex"]).decode("utf8", errors="replace")
            for item in items
        ]

        if minimum is not None:
            assert (
                len(messages) >= minimum
            ), f"Matching messages on topic '{topic}' is less than minimum.\nwanted: {minimum}\ngot: {len(messages)}\n\nmessages:\n{messages}"

        if maximum is not None:
            assert (
                len(messages) <= maximum
            ), f"Matching messages on topic '{topic}' is greater than maximum.\nwanted: {maximum}\ngot: {len(messages)}\n\nmessages:\n{messages}"

        return messages

    def log_mqtt_messages(
        self,
        topic: str = "#",
        date_from: Optional[timestamp.Timestamp] = None,
        **kwargs,
    ):
        """Log matching MQTT messages"""
        items = self.mqtt_match_messages(
            topic=topic,
            date_from=date_from,
            **kwargs,
        )

        # hide sensitive information
        # This is fragile and should be improved upon once a more suitable/robust method of logging and querying
        # the mqtt messages is found.
        entries = []
        for item in items:
            payload = bytes.fromhex(item["payload_hex"]).decode(
                "utf8", errors="replace"
            )
            if item["message"]["topic"] == C8Y_TOKEN_TOPIC and payload.startswith(
                "71,"
            ):
                payload = "71,<redacted>"
            entries.append(
                f'{item["message"]["tst"].replace("+0000", ""):32} {item["message"]["topic"]:70} {payload}'
            )
        log.info("---- mqtt messages ----\n%s", "\n".join(entries))

    @keyword("Should Have MQTT Messages")
    def mqtt_should_have_topic(
        self,
        topic: str,
        date_from: Optional[timestamp.Timestamp] = None,
        date_to: Optional[timestamp.Timestamp] = None,
        message_pattern: Optional[str] = None,
        message_contains: Optional[str] = None,
        minimum: int = 1,
        maximum: Optional[int] = None,
        **kwargs,
    ) -> List[str]:
        """
        Check for the presence of a topic

        Note: If any argument is set to None, then it will be ignored in the filtering.

        Args:
            topic (str): Filter by topic. Supports MQTT wildcard patterns
            date_from (timestamp.Timestamp, optional): Date from filter. Accepts
                relative or epoch time
            date_to (timestamp.Timestamp, optional): Date to filter. Accepts
                relative or epoch time
            message_pattern (str, optional): Only include MQTT messages matching a regular expression
            message_contains (str, optional): Only include MQTT messages containing a given string
            minimum (int, optional): Minimum number of message to expect. Defaults to 1
            maximum (int, optional): Maximum number of message to expect. Defaults to None

        *Examples:*

        | ${listen}=   | `Should Have MQTT Message` | topic=tedge/${CHILD_SN}/commands/req/config_snapshot | date_from=-5s |
        | ${messages}= | `Should Have MQTT Message` | te/device/main/service/tedge-agent/status/health | minimum=1 | minimum=2 |
        | ${messages}= | `Should Have MQTT Message` | te/device/main/service/tedge-agent/status/health | minimum=1 | minimum=2 | message_contains="time" |
        | ${messages}= | `Should Have MQTT Message` | te/device/main/service/tedge-agent/status/health | minimum=1 | minimum=2 | message_pattern="value":\\s*\\d+ |
        """
        result = self._assert_mqtt_topic_messages(
            topic,
            date_from=date_from,
            date_to=date_to,
            minimum=minimum,
            maximum=maximum,
            message_pattern=message_pattern,
            message_contains=message_contains,
            **kwargs,
        )
        return result

    @keyword("Should Not Have MQTT Messages")
    def mqtt_should_not_have_topic(
        self,
        topic: str,
        date_from: Optional[timestamp.Timestamp] = None,
        date_to: Optional[timestamp.Timestamp] = None,
        message_pattern: Optional[str] = None,
        message_contains: Optional[str] = None,
        wait_seconds: int = 2,
        **kwargs,
    ) -> List[str]:
        """
        Verify that NO MQTT messages exist matching the given criteria.

        This keyword asserts that there are zero messages on the specified topic
        that match the given filters. It will fail if any matching messages are found.

        IMPORTANT: This keyword waits for `wait_seconds` before checking messages to ensure
        that any in-flight messages have been published and logged. This prevents race
        conditions where messages are still being processed.

        Note: If any argument is set to None, then it will be ignored in the filtering.

        Args:
            topic (str): Filter by topic. Supports MQTT wildcard patterns
            date_from (timestamp.Timestamp, optional): Date from filter. Accepts
                relative or epoch time. If not specified, defaults to the test start time.
            date_to (timestamp.Timestamp, optional): Date to filter. Accepts
                relative or epoch time
            message_pattern (str, optional): Only include MQTT messages matching a regular expression
            message_contains (str, optional): Only include MQTT messages containing a given string
            wait_seconds (int, optional): Time in seconds to wait before checking messages.
                Defaults to 2 seconds. This ensures any in-flight messages have time to be
                published and logged.

        Returns:
            List[str]: Empty list (always returns empty list when assertion passes)

        *Examples:*

        Verify no messages on a specific topic:
        | `Should Not Have MQTT Messages` | topic=tedge/${CHILD_SN}/commands/req/config_snapshot |

        Verify no messages in a time window:
        | `Should Not Have MQTT Messages` | topic=te/device/main/service/tedge-agent/status/health | date_from=-5s |

        Verify no messages containing specific text:
        | `Should Not Have MQTT Messages` | topic=te/# | message_contains="error" |

        Verify no messages matching a pattern:
        | `Should Not Have MQTT Messages` | topic=te/# | message_pattern="status.*down" |

        Verify no messages with custom wait time:
        | `Should Not Have MQTT Messages` | topic=te/# | message_contains="error" | wait_seconds=5 |
        """
        # Wait to ensure any in-flight messages have been published and logged
        time.sleep(wait_seconds)

        result = self._assert_mqtt_topic_messages(
            topic,
            date_from=date_from,
            date_to=date_to,
            minimum=0,
            maximum=0,
            message_pattern=message_pattern,
            message_contains=message_contains,
            **kwargs,
        )
        return result

    @keyword("Should Have Retained MQTT Messages")
    def should_have_retained_mqtt_messages(
        self,
        topic: str,
        message_pattern: Optional[str] = None,
        message_contains: Optional[str] = None,
        device_name: Optional[str] = None,
    ) -> List[str]:
        """
        Check for a retained message on the given topic

        Args:
            topic (str): Filter by topic. Supports MQTT wildcard patterns

        *Example:*
        | ${messages}= | `Should Have Retained MQTT Messages` | te/device/child01/# |
        """
        device = self.get_device(device_name)

        command = f"tedge mqtt sub {topic} --retained-only --no-topic --duration 1s"
        output = device.execute_command(command).stdout
        lines = output.splitlines()

        if message_contains:
            message_pattern = r".*" + re.escape(message_contains) + r".*"

        message_pattern_re = None
        if message_pattern:
            message_pattern_re = re.compile(message_pattern, re.IGNORECASE)

        messages = []
        for line in lines:
            if message_pattern_re is None or message_pattern_re.match(line):
                messages.append(line)

        assert messages, "Expected at least one retained message, but received none"
        return messages

    @keyword("Should Not Have Retained MQTT Messages")
    def should_not_have_retained_mqtt_messages(
        self, topic: str, device_name: Optional[str] = None
    ):
        """
        Assert that there are no retained messages on the given topic

        Args:
            topic (str): Filter by topic. Supports MQTT wildcard patterns

        *Example:*
        | `Should Not Have Retained MQTT Messages` | te/device/child01/# |
        """
        device = self.get_device(device_name)

        output = ""
        for _ in range(5):
            command = f"tedge mqtt sub {topic} --retained-only --no-topic --duration 1s"
            output = device.execute_command(command).stdout
            if output == "":
                return ""
            time.sleep(1)
        raise AssertionError(f"Expected no messages, but received: {output}")

    @keyword("Register Child Device")
    def register_child(
        self,
        parent_name: str,
        child_name: str,
        supported_operations: Optional[Union[List[str], str]] = None,
    ):
        """
        Register a child device to a parent along with a given list of supported operations

        *Examples:*

        | `Register Child Device` | parent_name=tedge001 | child_name=child01 | supported_operations=c8y_LogfileRequest,c8y_SoftwareUpdate |
        """
        self.set_current(parent_name)
        device = self.get_device()
        cmd = [f"sudo mkdir -p '/etc/tedge/operations/c8y/{child_name}'"]

        if isinstance(supported_operations, str):
            supported_operations = supported_operations.split(",")

        if supported_operations:
            for op_type in supported_operations:
                cmd.append(
                    f"sudo touch '/etc/tedge/operations/c8y/{child_name}/{op_type}'"
                )

        device.assert_command(" && ".join(cmd))

    @keyword("Set Restart Command")
    def set_restart_command(self, command: str, **kwargs):
        """Set the restart command used by thin-edge.io to restart the device.

        *Examples:*
        | `Set Restart Command` | ["/sbin/reboot"] |
        | `Set Restart Command` | ["/usr/bin/on_shutdown.sh", "300"] |
        """
        self.execute_command(
            f"sed -i 's|reboot =.*|reboot = {command}|g' /etc/tedge/system.toml",
            **kwargs,
        )

    @keyword("Set Restart Timeout")
    def set_restart_timeout(self, value: Union[str, int], **kwargs):
        """Set the restart timeout interval in seconds for how long thin-edge.io
        should wait to for a device restart to happen.

        Use a value of "default" if you want to revert to the default thin-edge.io timeout setting.

        *Examples:*
        | `Set Restart Timeout` | 60 |
        | `Set Restart Timeout` | default |
        """
        if str(value).lower() == "default":
            command = "sed -i -e '/reboot_timeout_seconds/d' /etc/tedge/system.toml"
        else:
            command = f"sed -i -e '/reboot_timeout_seconds/d' -e '/reboot =/a reboot_timeout_seconds = {value}' /etc/tedge/system.toml"

        self.execute_command(command, **kwargs)

    @keyword("Escape Pattern")
    def regexp_escape(self, pattern: str, is_json: bool = False):
        """Escape a string for use in a regular expression with
        optional escaping for a json string (as json needs also needs
        to have back slashes escaped)

        Examples:
        | ${escaped} = | Escape Pattern | ${original} |
        | ${escaped} = | Escape Pattern | ${original} | is_json=${True} |
        """
        value = re.escape(pattern)

        if is_json:
            return value.replace("\\", "\\\\")

        return value

    @keyword("Add Remote Access Passthrough Configuration")
    def add_remote_access_passthrough_configuration(
        self, device: str = "", port: int = 22, **kwargs
    ) -> str:
        """Add the Cumulocity Remote Access Passthrough configuration
        to a device

        Examples:
        | ${stdout} = | Add Remote Access Passthrough Configuration |
        | ${stdout} = | Add Remote Access Passthrough Configuration | device=mycustomdevice |
        | ${stdout} = | Add Remote Access Passthrough Configuration | device=mycustomdevice | port=22222 |
        """
        if not device:
            device = self.get_device().get_id()

        assert shutil.which(
            "c8y"
        ), "could not find c8y binary. Check that go-c8y-cli is installed"
        cmd = [
            "c8y",
            "remoteaccess",
            "configurations",
            "create-passthrough",
            "--retries=5",
            "--device",
            device,
            f"--port={port}",
        ]

        env = {
            **os.environ.copy(),
            "CI": "true",
        }
        with subprocess.Popen(
            cmd,
            text=True,
            encoding="utf8",
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=env,
        ) as proc:
            timeout = kwargs.pop("timeout", 30)
            proc.wait(timeout)
            output = proc.stdout.read() if proc.stdout else ""
            assert (
                proc.returncode == 0
            ), f"Failed to add remote access PASSTHROUGH configuration.\n{output}"
            return output

    @keyword("Execute Remote Access Command")
    def execute_remote_access_command(
        self,
        command,
        device: str = "",
        key_file: str = "",
        user: str = "",
        exp_exit_code: int = 0,
        stdout: bool = True,
        stderr: bool = False,
        **kwargs,
    ) -> Union[str, Tuple[str]]:
        """Execute a command using the Cumulocity Remote Access feature (using ssh)

        You have to supply it a local ssh key (local to the machine running the tests).

        Examples:
        | ${stdout} = | Execute Remote Access Command | command=ls -l | key_file=/tmp/key |
        | ${stdout} = | Add Remote Access Passthrough Configuration | device=mycustomdevice |
        | ${stdout} = | Add Remote Access Passthrough Configuration | device=mycustomdevice | port=22222 |
        """
        if not device:
            device = self.get_device().get_id()

        assert shutil.which(
            "c8y"
        ), "could not find c8y binary. Check that go-c8y-cli is installed"
        cmd = [
            "c8y",
            "remoteaccess",
            "connect",
            "run",
            "-n",
            "--device",
            device,
            "--",
            "sh",
            "-c",
            rf"ssh -F /dev/null -o PasswordAuthentication=no -o StrictHostKeyChecking=no -o IdentitiesOnly=yes -i '{key_file}' -p %p {user}@%h -- {command}",
        ]

        env = {
            **os.environ.copy(),
            "CI": "true",
        }

        with subprocess.Popen(
            cmd,
            text=True,
            encoding="utf8",
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        ) as proc:
            timeout = kwargs.pop("timeout", 30)
            proc.wait(timeout)

            out = proc.stdout.read() if proc.stdout else ""
            err = proc.stderr.read() if proc.stderr else ""
            if exp_exit_code is not None:
                assert (
                    proc.returncode == exp_exit_code
                ), f"Failed to connect via remote access.\nstdout: <<EOT{out}\nEOT\n\nstderr <<EOT\n{err}\nEOT"

            log.info("Command:\n%s", out)

            output = []
            if stdout:
                output.append(out)
            if stderr:
                output.append(err)

            if len(output) == 0:
                return ""

            if len(output) == 1:
                return output[0]

            return tuple(output)

    @keyword("Configure SSH")
    def configure_ssh(self, user: str = "root", device: Optional[str] = None) -> str:
        """Configure SSH on a device.

        The keyword generates a new ssh key pair on the host running the test, then adds
        the public key to the authorized_keys on the device under test, then returns the path
        to the private key (so it can be used in subsequent calls)
        """
        if not device:
            device = self.get_device().get_id()

        key = Path("/tmp") / device
        pub_key = Path("/tmp") / f"{device}.pub"
        key.unlink(missing_ok=True)
        pub_key.unlink(missing_ok=True)

        # create ssh key
        subprocess.check_call(
            ["ssh-keygen", "-b", "2048", "-t", "rsa", "-f", key, "-q", "-N", ""]
        )

        # add public key to authorized_keys on the device
        ssh_dir = "/root/.ssh" if user == "root" else f"/home/{user}/.ssh"
        pub_key_encoded = base64.b64encode(pub_key.read_bytes()).decode("utf8")
        self.execute_command(
            f"mkdir -p {ssh_dir} && echo {pub_key_encoded} | base64 -d >> '{ssh_dir}/authorized_keys'"
        )
        return str(key)

    @keyword("Get Bridge Service Name")
    def get_bridge_service_name(self, cloud: str) -> str:
        """Get the name of the bridge service.

        The service name will depend if the built-in bridge
        has been activated or not (on the device).
        """
        output = self.execute_command(
            "tedge config get mqtt.bridge.built_in", strip=True, ignore_exit_code=True
        )
        if output == "true":
            return f"tedge-mapper-bridge-{cloud}"

        # Legacy mosquitto bridge
        return f"mosquitto-{cloud}-bridge"

    @keyword("Bridge Should Be Up")
    def bridge_should_be_up(self, cloud: str, **kwargs) -> Dict[str, Any]:
        """Assert that the bridge should be up/healthy

        Examples:
        | Bridge Should Be Up | c8y |
        | Bridge Should Be Up | aws |
        | Bridge Should Be Up | az |
        """
        return self.assert_service_health_status_up(
            self.get_bridge_service_name(cloud), **kwargs
        )

    @keyword("Bridge Should Be Down")
    def bridge_should_be_down(self, cloud: str, **kwargs) -> Dict[str, Any]:
        """Assert that the bridge should be down/unhealthy

        Examples:
        | Bridge Should Be Down | c8y |
        | Bridge Should Be Down | aws |
        | Bridge Should Be Down | az |
        """
        return self.assert_service_health_status_down(
            self.get_bridge_service_name(cloud), **kwargs
        )

    @keyword("Delete SmartREST 1.0 Template")
    def delete_smartrest_one_template(self, template_id: str):
        """Delete a SmartREST 1.0 template from Cumulocity

        Examples:
        | Delete SmartREST 1.0 Template | myCustomName |
        """
        try:
            mo_id = c8y_lib.c8y.identity.get_id(
                template_id, "c8y_SmartRestDeviceIdentifier"
            )
            log.info(
                "Deleting SmartREST 1.0 template. external_id=%s, managed_object_id=%s",
                template_id,
                mo_id,
            )
            c8y_lib.c8y.inventory.delete(mo_id)
        except Exception as ex:
            log.warning(
                "Could not deleted SmartREST 1.0 template. id=%s, ex=%s",
                template_id,
                ex,
            )

    @keyword("Register Entity")
    def register_entity(
        self,
        topic_id: str,
        type: str,
        parent: str = "device/main//",
        device_name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """
        Register the provided entity in the entity store

        Args:
            topic_id (str, optional): Topic ID of the new entity
            type (str, optional): Type of the new entity
            parent (str, optional): Topic ID of the parent
            device_name (str, optional): Device name to fetch the entity list from

        Returns:
            Dict[str, Any]: Registered entity topic ID

        *Example:*
        | ${entities}= | Register Entity | device/child0// | child-device |
        | ${entities}= | Register Entity | device/child0/service/service0 | service | device/child0// |
        | ${entities}= | Register Entity | device/child0/service/service0 | service | device/child0// | device_name=${PARENT_SN} |
        """
        device = self.current
        if device_name:
            if device_name in self.devices:
                device = self.devices.get(device_name)

        if not device:
            raise ValueError(
                f"Unable to query the entity store as the device: '{device_name}' has not been setup"
            )

        payload = {"@topic-id": topic_id, "@type": type, "@parent": parent}
        json_payload = json.dumps(payload)

        command = f"tedge http post /te/v1/entities --data '{json_payload}'"
        output = device.execute_command(command)
        json_output = json.loads(output.stdout) if output.stdout else {}
        return json_output

    @keyword("Get Entity")
    def get_entity(
        self, topic_id: str, device_name: Optional[str] = None
    ) -> Dict[str, Any]:
        """
        Get the entity from the entity store

        Args:
            topic_id (str, optional): Topic ID of the entity
            device_name (str, optional): Device name to fetch the entity list from

        Returns:
            Dict[str, Any]: Entity metadata

        *Example:*
        | ${entities}= | Get Entity | device/child0// |
        | ${entities}= | Get Entity | device/child0/service/service0 | device_name=${PARENT_SN} |
        """
        device = self.current
        if device_name:
            if device_name in self.devices:
                device = self.devices.get(device_name)

        if not device:
            raise ValueError(
                f"Unable to query the entity store as the device: '{device_name}' has not been setup"
            )

        command = f"curl http://localhost:8000/te/v1/entities/{topic_id}"
        output = device.execute_command(command)
        json_output = json.loads(output.stdout)
        return json_output

    @keyword("Deregister Entity")
    def deregister_entity(
        self, topic_id: str, device_name: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        """
        Delete the given entity and its child tree from the entity store

        Args:
            topic_id (str, optional): Topic ID of the entity to be deleted
            device_name (str, optional): Device name to perform this action from

        Returns:
            Dict[str, Any]: Registered entity topic ID

        *Example:*
        | ${entities}= | Delete Entity | device/child0// |
        | ${entities}= | Delete Entity | device/child0/service/service0 | device_name=${PARENT_SN} |
        """
        device = self.get_device(device_name)
        command = f"tedge http delete /te/v1/entities/{topic_id}"
        output = device.execute_command(command)
        json_output = json.loads(output.stdout) if output.stdout else []
        return json_output

    @keyword("List Entities")
    def list_entities(
        self,
        root: Optional[str] = None,
        parent: Optional[str] = None,
        type: Optional[str] = None,
        device_name: Optional[str] = None,
    ) -> List[Dict[str, Any]]:
        """
        Get entity list from the device using the entity store query REST API

        Args:
            root (str, optional): Topic id of the entity to start the search from
            parent (str, optional): Topic id of the entity that is the parent of the entities to list
            type (str, optional): Entity type: device, child-device or service
            device_name (str, optional): Device name to fetch the entity list from

        Returns:
            List[Dict[str, Any]]: List of entities

        *Example:*
        | ${entities}= | List Entities |
        | ${entities}= | List Entities | root=device/child01// |
        | ${entities}= | List Entities | parent=device/main// |
        | ${entities}= | List Entities | type=child-device |
        | ${entities}= | List Entities | root=device/main// | type=service |
        | ${entities}= | List Entities | parent=device/main// | type=child-device |
        | ${entities}= | List Entities | device_name=${PARENT_SN} |
        """
        device = self.current
        if device_name:
            if device_name in self.devices:
                device = self.devices.get(device_name)

        if not device:
            raise ValueError(
                f"Unable to query the entity store as the device: '{device_name}' has not been setup"
            )

        url = "/te/v1/entities"
        params = {}
        if root:
            params["root"] = root
        if parent:
            params["parent"] = parent
        if type:
            params["type"] = type

        if params:
            query_string = "&".join(f"{key}={value}" for key, value in params.items())
            url += f"?{query_string}"

        output = device.execute_command(f"tedge http get '{url}'")
        entities = json.loads(output.stdout) if output.stdout else []
        return entities

    @keyword("Should Contain Entity")
    def assert_contains_entity(
        self,
        item: Union[str, Dict[str, Any]],
        entities: Optional[List[Dict[str, Any]]] = None,
        device_name: Optional[str] = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
        """Assert if the entity store contains the given entity

        Args:
            entity (str or Dict[str, Any]]): Entity to look for
            entities (List[Dict[str, Any]], optional): List of entities to search in. Defaults to None.
            device_name (str, optional): Device name to fetch the entity list from

        Returns:
            List[Dict[str, Any]]: List of entities matching the given entity definition

        *Example:*
        | ${entities}= | Should Contain Entity | item=${entity_json} |
        | ${entities}= | Should Contain Entity | item=${entity_json} | entities=${entity_list_json} |
        | ${entities}= | Should Contain Entity | item=${entity_json} | entities=${entity_list_json} | device_name=${PARENT_SN} |
        """
        device = self.current
        if device_name:
            if device_name in self.devices:
                device = self.devices.get(device_name)

        if not device:
            raise ValueError(
                f"Unable to query the entity store as the device: '{device_name}' has not been setup"
            )

        if not entities:
            entities = self.list_entities()

        if isinstance(item, str):
            item = json.loads(item)

        matches = entities
        if item:
            matches = [entity for entity in entities if entity == item]

        assert matches

        return matches

    @keyword("Should Not Contain Entity")
    def assert_does_not_contain_entity(
        self,
        topic_id: str,
        entities: Optional[List[Dict[str, Any]]] = None,
        device_name: Optional[str] = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
        """Assert that the entity store does not contains the given entity

        Args:
            topic_id (str, optional): Topic ID of the entity
            entities (List[Dict[str, Any]], optional): List of entities to search in. Defaults to None.
            device_name (str, optional): Device name to fetch the entity list from

        Returns:
            List[Dict[str, Any]]: List of entities matching the given entity definition

        *Example:*
        | ${entities}= | Should Not Contain Entity | topic_id=device/child123// |
        | ${entities}= | Should Not Contain Entity | topic_id=device/child123// | entities=${entity_list_json} |
        | ${entities}= | Should Not Contain Entity | topic_id=device/child123// | entities=${entity_list_json} | device_name=${PARENT_SN} |
        """
        if not entities:
            entities = self.list_entities(device_name=device_name)

        assert all(entity["@topic-id"] != topic_id for entity in entities)

        return entities


def mqtt_topic_match(m: matcher.MQTTMatcher, topic: str) -> bool:
    """check if an MQTT topic matches

    Args:
        matcher (matcher.MQTTMatcher): MQTT pattern
        topic (str): topic to match against

    Returns:
        bool: Topic matches the given pattern
    """
    try:
        next(m.iter_match(topic))
        return True
    except StopIteration:
        return False
