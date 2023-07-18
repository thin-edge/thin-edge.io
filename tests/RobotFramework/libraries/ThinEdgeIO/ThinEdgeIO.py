"""ThinEdgeIO Library for Robot Framework

It enables the creation of devices which can be used in tests.
It currently support the creation of Docker devices only
"""
# pylint: disable=invalid-name

import logging
import json
from typing import Any, Union, List, Dict
import time
from datetime import datetime
import re

import dateparser
from paho.mqtt import matcher
from robot.api.deco import keyword, library
from DeviceLibrary import DeviceLibrary, DeviceAdapter
from Cumulocity import Cumulocity, retry


relativetime_ = Union[datetime, str]


devices_lib = DeviceLibrary()
c8y_lib = Cumulocity()


logging.basicConfig(
    level=logging.DEBUG, format="%(asctime)s %(module)s -%(levelname)s- %(message)s"
)
log = logging.getLogger(__name__)

__version__ = "0.0.1"
__author__ = "Reuben Miller"


class MQTTMessage:
    timestamp: float
    topic: str
    qos: str
    retain: int
    payloadlen: int
    payload: str

    @property
    def date(self) -> datetime:
        return datetime.fromtimestamp(self.timestamp)

    # {"tst":"2022-12-27T16:55:44.923776Z+0000","topic":"c8y/s/us","qos":0,"retain":0,"payloadlen":99,"payload":"119,c8y-bridge.conf,c8y-configuration-plugin,example,mosquitto.conf,tedge-mosquitto.conf,tedge.toml"}


@library(scope="SUITE", auto_keywords=False)
class ThinEdgeIO(DeviceLibrary):
    """ThinEdgeIO Library"""

    def __init__(
        self,
        image: str = DeviceLibrary.DEFAULT_IMAGE,
        adapter: str = None,
        bootstrap_script: str = DeviceLibrary.DEFAULT_BOOTSTRAP_SCRIPT,
        **kwargs,
    ):
        super().__init__(
            image=image, adapter=adapter, bootstrap_script=bootstrap_script, **kwargs
        )

        # Configure retries
        retry.configure_retry_on_members(self, "^_assert_")

    def end_suite(self, _data: Any, result: Any):
        """End suite hook which is called by Robot Framework
        when the test suite has finished

        Args:
            _data (Any): Test data
            result (Any): Test details
        """
        log.info("Suite %s (%s) ending", result.name, result.message)

        for device in self.devices.values():
            try:
                if isinstance(device, DeviceAdapter):
                    if device.should_cleanup:
                        self.remove_certificate_and_device(device)
            except Exception as ex:
                log.warning("Could not cleanup certificate/device. %s", ex)

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

        # TODO: Only cleanup on the suite?
        # self.remove_certificate_and_device(self.current)
        super().end_test(_data, result)

    @keyword("Get Debian Architecture")
    def get_debian_architecture(self):
        """Get the debian architecture"""
        return self.execute_command(
            "dpkg --print-architecture", strip=True, stdout=True, stderr=False
        )

    @keyword("Get Logs")
    def get_logs(self, name: str = None, date_from: Union[datetime, float] = None, show=True):
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
            return

        device_sn = name or device.get_id()
        try:
            # TODO: optionally check if the device was registered or not, if no, then skip this step
            managed_object = c8y_lib.device_mgmt.identity.assert_exists(
                device_sn, timeout=5
            )
            log.info(
                "Managed Object\n%s", json.dumps(managed_object.to_json(), indent=2)
            )
            self.log_operations(managed_object.id)
        except Exception as ex:  # pylint: disable=broad-except
            log.warning("Failed to get device managed object. %s", ex)

        try:
            # Get agent log files (if they exist)
            log.info("tedge agent logs: /var/log/tedge/agent/*")
            device.execute_command(
                "tail -n +1 /var/log/tedge/agent/* 2>/dev/null || true",
                shell=True,
            )
        except Exception as ex:
            log.warning("Failed to retrieve logs. %s", ex, exc_info=True)

        super().get_logs(device.get_id(), date_from=date_from, show=show)

    def log_operations(self, mo_id: str, status: str = None):
        """Log operations to help with debugging

        Args:
            mo_id (str): Managed object id
            status (str, optional): Operation status. Defaults to None (which means all statuses).
        """
        operations = c8y_lib.c8y.operations.get_all(
            device_id=mo_id, status=status, after=self.test_start_time
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

    def remove_certificate_and_device(self, device: DeviceAdapter = None):
        """Remove trusted certificate"""
        if device is None:
            device = self.current

        if not device:
            log.info(f"No certificate to remove as the device as not been set")
            return

        result = device.execute_command(
            "command -v tedge >/dev/null && (tedge cert show | grep '^Thumbprint:' | cut -d' ' -f2 | tr A-Z a-z) || true",
        )
        if result.return_code != 0:
            log.info("Failed to get device certificate fingerprint. %s", result.stdout)
            return

        fingerprint = result.stdout.strip()
        if fingerprint:
            try:
                c8y_lib.trusted_certificate_delete(fingerprint)

                device_sn = device.get_id()
                if device_sn:
                    c8y_lib.delete_managed_object(device_sn)
                else:
                    log.info(
                        "Device serial number is empty, so nothing to delete from Cumulocity"
                    )
            except Exception as ex:
                log.warning(
                    "Could not remove device certificate and/or device. error=%s", ex
                )

    @keyword("Download From GitHub")
    def download_from_github(self, *run_id: str, arch: str = "aarch64"):
        """Download artifacts from a GitHub Run

        Args:
            *run_id (str): Run ids of the artifacts to download
            arch (str, optional): CPU Architecture to download for. Defaults to aarch64
        """

        # pylint: disable=line-too-long
        self.execute_command(
            """
            type -p curl >/dev/null || sudo apt install curl -y
            curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \\
            && sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \\
            && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null \\
            && sudo apt-get update \\
            && sudo apt-get -y install gh
        """.lstrip()
        )

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
            self.execute_command(
                f"""
                gh run download {i_run_id} -n debian-packages-{arch}-unknown-linux-gnu -R thin-edge/thin-edge.io
            """.strip()
            )

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
        message_pattern: str = None,
        date_from: relativetime_ = None,
        date_to: relativetime_ = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
        """Match mqtt messages using different types of filters

        Args:
            topic (str): Filter by topic

        """
        cmd = "journalctl -u mqtt-logger.service -n 1000 --output=cat"

        if not date_from:
            date_from = self.test_start_time

        cmd += f" --since '@{to_date(date_from).timestamp()}'"

        if date_to:
            cmd += f" --until '@{to_date(date_to).timestamp()}'"

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
    def assert_service_health_status_up(self, service: str) -> Dict[str, Any]:
        """Checks if the Service Health Status is up

        *Example:*
        | `Service Health Status Should Be Up` | | | | | tedge-mapper-c8y |
        """
        return self._assert_health_status(service, status="up")

    @keyword("Service Health Status Should Be Down")
    def assert_service_health_status_down(self, service: str) -> Dict[str, Any]:
        return self._assert_health_status(service, status="down")

    @keyword("Service Health Status Should Be Equal")
    def assert_service_health_status_equal(
        self, service: str, status: str
    ) -> Dict[str, Any]:
        return self._assert_health_status(service, status=status)

    def _assert_health_status(self, service: str, status: str) -> Dict[str, Any]:
        # if mqtt.client.auth.ca_file or mqtt.client.auth.ca_dir is set, we pass setting
        # value to mosquitto_sub
        mqtt_config_options = self.execute_command(
            f"tedge config list", stdout=True, stderr=False, ignore_exit_code=True
        )

        server_auth = ""
        if "mqtt.client.auth.ca_file" in mqtt_config_options:
            server_auth = "--cafile /etc/mosquitto/ca_certificates/ca.crt"

        client_auth = ""
        if "mqtt.client.auth.cert_file" in mqtt_config_options:
            client_auth = "--cert /setup/client.crt --key /setup/client.key"

        message = self.execute_command(
            f"mosquitto_sub -t 'tedge/health/{service}' --retained-only -C 1 -W 5 -p $(tedge config get mqtt.client.port) {server_auth} {client_auth}",
            stdout=True,
            stderr=False,
        )
        current_status = ""
        try:
            health = json.loads(message)
            current_status = health.get("status", "")
        except Exception as ex:
            log.debug(
                "Detected non json health message. json_error=%s, message=%s",
                ex,
                message,
            )
            current_status = message
        assert current_status == status
        return health

    @keyword("Setup")
    def setup(
        self,
        skip_bootstrap: bool = None,
        cleanup: bool = None,
        adapter: str = None,
        wait_for_healthy: bool = True,
    ) -> str:
        serial_sn = super().setup(skip_bootstrap, cleanup, adapter)

        if not skip_bootstrap and wait_for_healthy:
            self.assert_service_health_status_up("tedge-mapper-c8y")
        return serial_sn

    def _assert_mqtt_topic_messages(
        self,
        topic: str,
        date_from: relativetime_ = None,
        date_to: relativetime_ = None,
        minimum: int = 1,
        maximum: int = None,
        message_pattern: str = None,
        message_contains: str = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
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
            bytes.fromhex(item["payload_hex"]).decode("utf8")
            # item["message"]["payload"]
            for item in items
        ]

        if minimum is not None:
            assert (
                len(messages) >= minimum
            ), f"Matching messages is less than minimum.\nwanted: {minimum}\ngot: {len(messages)}\n\nmessages:\n{messages}"

        if maximum is not None:
            assert (
                len(messages) <= maximum
            ), f"Matching messages is greater than maximum.\nwanted: {maximum}\ngot: {len(messages)}\n\nmessages:\n{messages}"

        return messages

    @keyword("Should Have MQTT Messages")
    def mqtt_should_have_topic(
        self,
        topic: str,
        date_from: relativetime_ = None,
        date_to: relativetime_ = None,
        message_pattern: str = None,
        message_contains: str = None,
        minimum: int = 1,
        maximum: int = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
        """
        Check for the presence of a topic

        Note: If any argument is set to None, then it will be ignored in the filtering.

        Args:
            topic (str): Filter by topic. Supports MQTT wildcard patterns
            date_from (relativetime_, optional): Date from filter. Accepts
                relative or epoch time
            date_to (relativetime_, optional): Date to filter. Accepts
                relative or epoch time
            message_pattern (str, optional): Only include MQTT messages matching a regular expression
            message_contains (str, optional): Only include MQTT messages containing a given string
            minimum (int, optional): Minimum number of message to expect. Defaults to 1
            maximum (int, optional): Maximum number of message to expect. Defaults to None

        *Examples:*

        | ${listen}= | `Should Have MQTT Message` | topic=tedge/${CHILD_SN}/commands/req/config_snapshot | date_from=-5s |
        | ${messages}= | `Should Have MQTT Message` | tedge/health/c8y-log-plugin | minimum=1 | minimum=2 |
        | ${messages}= | `Should Have MQTT Message` | tedge/health/c8y-log-plugin | minimum=1 | minimum=2 | message_contains="time" |
        | ${messages}= | `Should Have MQTT Message` | tedge/health/c8y-log-plugin | minimum=1 | minimum=2 | message_pattern="value":\s*\d+ |
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


def to_date(value: relativetime_) -> datetime:
    if isinstance(value, datetime):
        return value
    if isinstance(value, (int, float)):
        return datetime.fromtimestamp(value)
    return dateparser.parse(value)


def mqtt_topic_match(matcher, topic) -> bool:
    try:
        next(matcher.iter_match(topic))
        return True
    except StopIteration:
        return False
