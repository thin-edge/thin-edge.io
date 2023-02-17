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
        return self.execute_command("dpkg --print-architecture", strip=True)

    @keyword("Get Logs")
    def get_logs(self, name: str = None):
        """Get device logs (override base class method to add additional debug info)

        Args:
            name (str, optional): Device name to get logs for. Defaults to None.
        """
        if not self.current:
            log.info("Device has not been setup, so no logs to collect")
            return

        device_sn = name or self.current.get_id()
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
            self.current.execute_command(
                "tail -n +1 /var/log/tedge/agent/* 2>/dev/null || true",
                shell=True,
            )

            # log.info("mqtt logs: mqtt-logger.service")
            # self.current.execute_command(
            #     f"journalctl -u mqtt-logger.service -n 1000 --since '@{int(self.test_start_time.timestamp())}'",
            #     shell=True,
            # )
        except Exception as ex:
            log.warning("Failed to retrieve logs. %s", ex, exc_info=True)

        super().get_logs(name)

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

        exit_code, fingerprint = device.execute_command(
            "command -v tedge >/dev/null && (tedge cert show | grep '^Thumbprint:' | cut -d' ' -f2 | tr A-Z a-z) || true",
        )
        if exit_code != 0:
            log.info("Failed to get device certificate fingerprint. %s", fingerprint)
            return

        fingerprint = fingerprint.decode("utf8").strip()
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
        """Dowload artifacts from a GitHub Run

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

        Returns:
            str: Command output
        """
        return self.execute_command(f"tedge config set {name} {value}")

    @keyword("Connect Mapper")
    def tedge_connect(self, mapper: str = "c8y") -> str:
        """Tedge connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".

        Returns:
            str: Command output
        """
        return self.execute_command(f"tedge connect {mapper}")

    @keyword("Disconnect Mapper")
    def tedge_disconnect(self, mapper: str = "c8y") -> str:
        """Tedge connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".

        Returns:
            str: Command output
        """
        return self.execute_command(f"tedge disconnect {mapper}")

    @keyword("Disconnect Then Connect Mapper")
    def tedge_disconnect_connect(self, mapper: str = "c8y", sleep: float = 0.0):
        """Tedge disconnect the connect a cloud

        Args:
            mapper (str, optional): Mapper name, e.g. c8y, az, etc. Defaults to "c8y".
            sleep (float, optional): Time to wait in seconds before connecting. Defaults to 0.0.
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

        output = self.execute_command(cmd, log_output=False)

        messages = []
        message_pattern_re = None
        if message_pattern:
            message_pattern_re = re.compile(message_pattern, re.IGNORECASE)

        for line in output.splitlines():
            try:
                message = json.loads(line)
                if "message" in message:
                    if message_pattern_re is None or message_pattern_re.match(message):
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

    def _assert_mqtt_topic_messages(
        self,
        topic: str,
        date_from: relativetime_ = None,
        date_to: relativetime_ = None,
        minimum: int = 1,
        maximum: int = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
        # log.info("Checking mqtt messages for topic: %s", topic)
        items = self.mqtt_match_messages(
            topic=topic, date_from=date_from, date_to=date_to, **kwargs
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
        minimum: int = 1,
        maximum: int = None,
        **kwargs,
    ) -> List[Dict[str, Any]]:
        """
        Check for the presence of a topic

        Examples

        | ${messages}= | Should Have MQTT Message | c8y/s/# | -30s | 0s |
        """
        result = self._assert_mqtt_topic_messages(
            topic,
            date_from=date_from,
            date_to=date_to,
            minimum=minimum,
            maximum=maximum,
            **kwargs,
        )
        return result


def to_date(value: relativetime_) -> datetime:
    if isinstance(value, datetime):
        return value

    return dateparser.parse(value)


def mqtt_topic_match(matcher, topic) -> bool:
    try:
        next(matcher.iter_match(topic))
        return True
    except StopIteration:
        return False
