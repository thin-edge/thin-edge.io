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
C8Y_TOKEN_TOPIC = "c8y/s/dat"


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

    def should_delete_device_certificate(self) -> bool:
        """Check if the certificate should be deleted or not
        """
        # Only delete the certificate if it is a self signed certificate

        # Parse the certificate details via tedge cert show
        lines = []
        try:
            lines = self.execute_command("tedge cert show", ignore_exit_code=True).splitlines()
        except Exception as ex:
            # Ignore any errors
            log.info("Could not read certificate information. %s", ex)

        # Prase output and decode the certificate information
        # Use simple parser to avoid having to decode the certificate
        certificate = {}
        for line in lines:
            key, _, value = line.partition(":")
            if key and value:
                certificate[key.lower().strip()] = value.strip()

        issuer = certificate.get("issuer", None)
        subject = certificate.get("subject", None)

        if issuer is None or subject is None:
            return False

        # Self signed certificates generally have the same issue information as the subject
        is_self_signed = subject == issuer
        return is_self_signed

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
                        if self.should_delete_device_certificate():
                            self.remove_certificate(device)
                        self.remove_device(device)
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

    @keyword("Create Device Profile")
    def create_device_profile(
        self,
        profile_id: str,
        profile_name: str,
        **kwargs,
    ) -> Dict[str, Any]:
        """Create a new device profile in Cumulocity

        Args:
            profile_id (str): Unique identifier for the device profile
            profile_name (str): Name of the device profile

        Example:
        | Command                                             |
        | Create Device Profile    profile_Id    profile_Name |

        Returns:
            Dict[str, Any]: The created device profile as a managed object
        """
        profile_body = {
            "type": "c8y_Profile",
            "name": profile_name,
            "c8y_DeviceProfile": {},
            "c8y_Filter": {"profileId": profile_id},
        }
        
        url = f"{c8y_lib.c8y.base_url}/inventory/managedObjects"
        
        response = c8y_lib.c8y.session.post(
            url,
            json=profile_body
        )
        response.raise_for_status()
        
        return response.json()

    @keyword("Get Debian Architecture")
    def get_debian_architecture(self):
        """Get the debian architecture"""
        return self.execute_command(
            "dpkg --print-architecture", strip=True, stdout=True, stderr=False
        )

    @keyword("Get Logs")
    def get_logs(
            self, name: str = None, date_from: Union[datetime, float] = None, show=True
    ):
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
            managed_object = c8y_lib.c8y.identity.get_object(device_sn, "c8y_Serial")
            log.info(
                "Managed Object\n%s", json.dumps(managed_object.to_json(), indent=2)
            )
            self.log_operations(managed_object.id)
        except KeyError:
            log.info("Skip getting managed object as it has not been registered in Cumulocity")
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
            # tedge-mapper-c8y message log (if they exist)
            log.info("tedge-mapper-c8y message log: /etc/tedge/.tedge-mapper-c8y/entity_store.jsonl")
            device.execute_command(
                "cat /etc/tedge/.tedge-mapper-c8y/entity_store.jsonl || true",
                shell=True,
            )
        except Exception as ex:
            log.warning("Failed to retrieve logs. %s", ex, exc_info=True)

        log_output = super().get_logs(device.get_id(), date_from=date_from, show=False)
        if show:
            hide_sensitive = self._hide_sensitive_factory()
            for line in log_output:
                print(hide_sensitive(line))

    def _hide_sensitive_factory(self):
        # This is fragile and should be improved upon once a more suitable/robust method of logging and querying
        # the mqtt messages is found.
        token_replace_pattern = re.compile(r"\{.+$")

        def _hide(line: str) -> str:
            if C8Y_TOKEN_TOPIC in line and "71," in line:
                line_sensitive = token_replace_pattern.sub(
                    f"(redacted log entry): Received token: topic={C8Y_TOKEN_TOPIC}, message=71,<redacted>", line)
                return line_sensitive
            return line

        return _hide

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

    def remove_certificate(self, device: DeviceAdapter = None):
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
            except Exception as ex:
                log.warning(
                    "Could not remove device certificate. error=%s", ex
                )

    def remove_device(self, device: DeviceAdapter = None):
        """Remove device from the cloud"""
        if device is None:
            device = self.current

        if not device:
            log.info(f"No device to remove as device context is not set")
            return

        try:
            device_sn = device.get_id()
            if not device_sn:
                log.info(
                    "Device serial number is empty, so nothing to delete from Cumulocity"
                )
                return
            device_mo = c8y_lib.c8y.identity.get_object(device_sn, "c8y_Serial")
            c8y_lib.device_mgmt.inventory.delete_device_and_user(device_mo)
        except KeyError:
            log.info("Device does not exist in cloud, nothing to delete")
        except Exception as ex:
            log.warning(
                "Could not remove device. error=%s", ex
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
    def assert_service_health_status_up(self, service: str, device: str = "main") -> Dict[str, Any]:
        """Checks if the Service Health Status is up

        *Examples:*

        | `Service Health Status Should Be Up` | tedge-mapper-c8y |
        | `Service Health Status Should Be Up` | tedge-mapper-c8y | device=child01 |
        """
        return self._assert_health_status(service, status="up", device=device)

    @keyword("Service Health Status Should Be Down")
    def assert_service_health_status_down(self, service: str, device: str = "main") -> Dict[str, Any]:
        return self._assert_health_status(service, status="down", device=device)

    @keyword("Service Health Status Should Be Equal")
    def assert_service_health_status_equal(
            self, service: str, status: str, device: str = "main"
    ) -> Dict[str, Any]:
        return self._assert_health_status(service, status=status, device=device)

    def _assert_health_status(self, service: str, status: str, device: str = "main") -> Dict[str, Any]:
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
            f"mosquitto_sub -t 'te/device/{device}/service/{service}/status/health' --retained-only -C 1 -W 5 -h $(tedge config get mqtt.client.host) -p $(tedge config get mqtt.client.port) {server_auth} {client_auth}",
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

    def log_mqtt_messages(self, topic: str = "#", date_from: Union[datetime, float] = None, **kwargs):
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
            payload = bytes.fromhex(item["payload_hex"]).decode("utf8", errors="replace")
            if item["message"]["topic"] == C8Y_TOKEN_TOPIC and payload.startswith("71,"):
                payload = "71,<redacted>"
            entries.append(f'{item["message"]["tst"].replace("+0000", ""):32} {item["message"]["topic"]:70} {payload}')
        log.info("---- mqtt messages ----\n%s", "\n".join(entries))

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

    @keyword("Register Child Device")
    def register_child(
            self,
            parent_name: str,
            child_name: str,
            supported_operations: Union[List[str], str] = None,
            name: str = None,
    ):
        """
        Register a child device to a parent along with a given list of supported operations

        *Examples:*

        | `Register Child Device` | parent_name=tedge001 | child_name=child01 | supported_operations=c8y_LogfileRequest,c8y_SoftwareUpdate |
        """
        self.set_current(parent_name)
        device = self.current
        cmd = [f"sudo mkdir -p '/etc/tedge/operations/c8y/{child_name}'"]

        if isinstance(supported_operations, str):
            supported_operations = supported_operations.split(",")

        if supported_operations:
            for op_type in supported_operations:
                cmd.append(f"sudo touch '/etc/tedge/operations/c8y/{child_name}/{op_type}'")

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
        if str(value).lower() == 'default':
            command = "sed -i -e '/reboot_timeout_seconds/d' /etc/tedge/system.toml"
        else:
            command = f"sed -i -e '/reboot_timeout_seconds/d' -e '/reboot =/a reboot_timeout_seconds = {value}' /etc/tedge/system.toml",
        self.execute_command(command, **kwargs, )

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
