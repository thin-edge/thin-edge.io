#!/usr/bin/env python3
"""thin-edge.io benchmark"""

import argparse
import datetime
import json
import logging
import random
import re
import sys
import shutil
import subprocess
import time
from enum import Enum
from itertools import cycle
from multiprocessing import Pool
import multiprocessing
from pathlib import Path
from typing import List, Union
import paho.mqtt.client as mqtt

# Set sensible logging defaults
LOG = logging.getLogger()
LOG.setLevel(logging.WARNING)
handler = logging.StreamHandler()
handler.setLevel(logging.WARNING)
formatter = logging.Formatter("%(asctime)s - %(name)s - %(levelname)s - %(message)s")
handler.setFormatter(formatter)
LOG.addHandler(handler)


class TelemetryType(Enum):
    MEASUREMENT = "measurement"
    ALARM = "alarm"
    EVENT = "event"

    def __str__(self) -> str:
        return str(self.value)

    @classmethod
    def values(cls) -> List[str]:
        return [item.value for item in cls]

    @classmethod
    def from_str(
        cls, value: str, default_value: "TelemetryType" = None
    ) -> "TelemetryType":
        for item in cls:
            if str(item.value).casefold() == value.casefold():
                return item

        return default_value


class Pub:
    def __init__(
        self,
        topic_id: str = "/",
        topic_root: str = "te",
        host: str = "127.0.0.1",
        port: int = 1883,
        qos: int = 0,
        count: int = 5,
        type_name: str = "",
        datapoints: int = 3,
        beats: int = 1,
        beats_delay: float = 0,
        period: float = 0,
        telemetry_type: TelemetryType = TelemetryType.MEASUREMENT,
    ) -> None:
        self._topic_root = topic_root
        self._topic_id = topic_id
        self.type_name = type_name
        self.qos = qos
        self.count = count
        self.host = host
        self.port = port

        self.beats = beats
        self.beats_delay = beats_delay
        self.period = period

        self.datapoints = datapoints
        self.telemetry_type = telemetry_type

        self.__ready = False
        self.__c8y_messages = []
        self.start_time = None

    @property
    def topic_prefix(self) -> str:
        return "/".join([self._topic_root, self._topic_id])

    def get_topic(self, telemetry_type: TelemetryType, type_name: str) -> str:
        sep = "/"
        telemetry_topics = {
            TelemetryType.MEASUREMENT: "m",
            TelemetryType.EVENT: "e",
            TelemetryType.ALARM: "a",
        }
        return sep.join(
            [self.topic_prefix, telemetry_topics[self.telemetry_type], type_name]
        )

    def __on_connect(self, client: mqtt.Client, userdata, flags, rc):
        if rc != 0:
            raise Exception("Failed to connect to broker")

        # observe which messages are being sent
        LOG.info("Subscribing to cloud topic")
        topics = {
            TelemetryType.MEASUREMENT: "c8y/measurement/measurements/create",
            TelemetryType.ALARM: "c8y/alarm/alarms/create",
            TelemetryType.EVENT: "c8y/event/events/create",
        }
        topic = topics[self.telemetry_type]
        client.subscribe(
            [
                (topic, 0),
            ]
        )
        self.__ready = True

    def __on_message(self, _client, _userdata, msg: mqtt.MQTTMessage):
        try:
            LOG.debug("Received message: %s", msg.payload)
            payload = json.loads(msg.payload.decode("utf-8"))
            if msg.topic == "c8y/measurement/measurements/create":
                if "msgid" in payload:
                    self.__c8y_messages.append(payload)
            elif msg.topic == "c8y/event/events/create":
                if "msgid" in payload:
                    self.__c8y_messages.append(payload)
            elif msg.topic == "c8y/alarm/alarms/create":
                if "msgid" in payload:
                    self.__c8y_messages.append(payload)
        except Exception as ex:
            LOG.debug("Could not parse payload. %s", ex)

    def wait_until_finished(self, wait: float = 5.0):
        expire = time.monotonic() + wait
        timeout = False
        while len(self.__c8y_messages) < self.count:
            time.sleep(0.1)

            if time.monotonic() > expire:
                timeout = True
                break

        return timeout

    def wait_until_ready(self, wait: float = 5.0):
        """Wait until the test is ready

        Args:
            wait (float): Time to wait in seconds for the benchmark client to be ready
        """
        expire = time.monotonic() + wait
        timeout = False
        while not self.__ready:
            time.sleep(0.1)

            if time.monotonic() > expire:
                timeout = True
                break

        if timeout:
            raise RuntimeError(f"Benchmark client is not ready after {wait} seconds")

    def run(self, procID, *args, **kwargs):
        client = mqtt.Client()
        client.on_connect = self.__on_connect
        client.on_message = self.__on_message
        client.max_inflight_messages_set(self.count)

        client.connect(self.host, self.port)
        client.loop_start()
        self.wait_until_ready()

        self.start_time = datetime.datetime.utcnow()

        # Build fixed payload
        if self.telemetry_type == TelemetryType.MEASUREMENT:
            payload_fixed = {
                f"data_{i}": round(random.uniform(20, 30), 2)
                for i in range(self.datapoints)
            }
        elif self.telemetry_type == TelemetryType.EVENT:
            payload_fixed = {
                f"data_{i}": round(random.uniform(20, 30), 2)
                for i in range(self.datapoints)
            }
            payload_fixed["text"] = "Test event"
        elif self.telemetry_type == TelemetryType.ALARM:
            payload_fixed = {
                f"data_{i}": round(random.uniform(20, 30), 2)
                for i in range(self.datapoints)
            }
            payload_fixed["text"] = "Test alarm"

        payload_bytes = 0

        burst_beats = self.beats
        beat_delay = self.beats_delay
        period = self.period

        idle_time_ms = 0
        beat_start = time.monotonic()
        beat_end = 0

        burst_warning_issued = False

        for i in range(1, self.count + 1):
            payload = json.dumps(
                {
                    "msgid": i,
                    "_generatedAt": datetime.datetime.utcnow().isoformat() + "Z",
                    **payload_fixed,
                }
            )

            payload_bytes += len(payload.encode("utf-8"))
            topic = self.get_topic(self.telemetry_type, self.type_name)
            LOG.debug("Publishing message: topic=%s, payload=%s", topic, payload)
            client.publish(topic, qos=self.qos, payload=payload)

            if i % burst_beats == 0:
                beat_end = time.monotonic()

                # Calculate remaining delay = period - total beat time
                remaining_delay = period / 1000 - (beat_end - beat_start)

                # Burst complete, wait between burst
                if remaining_delay > 0:
                    LOG.debug(
                        "Waiting remaining time in period: %0.3fs", remaining_delay
                    )
                    idle_time_ms += remaining_delay * 1000
                    time.sleep(remaining_delay)
                else:
                    # Only issue period exceeded warning once per iteration to prevent spamming logs
                    if not burst_warning_issued:
                        LOG.warning(
                            "Burst time is exceeding the period. Skipping delay. diff=%0.3f",
                            remaining_delay,
                        )
                        burst_warning_issued = True

                beat_start = time.monotonic()
            else:
                # Delay between beats inside a burst
                if beat_delay > 0:
                    LOG.debug("burst beat delay: %0.2fms", beat_delay)
                    idle_time_ms += beat_delay
                    time.sleep(beat_delay / 1000)

        end_time = datetime.datetime.utcnow()
        delta = end_time - self.start_time

        idle_sec = round(idle_time_ms / 1000, 3)

        # Wait for messages to finish publishing, give max 5 seconds to complete
        LOG.info("Waiting for last message to be published")
        self.wait_until_finished(2.0)

        LOG.info("Stopping mqtt client")
        client.disconnect()
        client.loop_stop()

        if self.telemetry_type == TelemetryType.MEASUREMENT:
            # thin-edge.io converts the msg id to a nested property
            total_cloud_messages = len(
                {
                    msg["msgid"]["msgid"]["value"]: msg
                    for msg in self.__c8y_messages
                    if msg.get("msgid", {}).get("msgid", {}).get("value", None)
                    is not None
                }
            )
        else:
            # Every other telemetry type should leave the msgid untouched
            total_cloud_messages = len(
                {
                    msg["msgid"]: msg
                    for msg in self.__c8y_messages
                    if msg.get("msgid", None) is not None
                }
            )

        dropped_messages = self.count - total_cloud_messages

        return {
            "worker": procID,
            "messages": self.count,
            "dropped_percent": round(dropped_messages * 100 / self.count, 2),
            "dropped_messages": dropped_messages,
            "total": delta.total_seconds(),
            "total_non_idle": round(delta.total_seconds() - idle_sec, 5),
            "idle": idle_sec,
            "parameters": {
                "count": self.count,
                "beats": self.beats,
                "beats_delay": self.beats_delay,
                "period": self.period,
            },
            "qos": self.qos,
            "total_payload_bytes": payload_bytes,
            "total_payload_bytes_per_message": payload_bytes / self.count,
            "messages_per_second": round(self.count / delta.total_seconds(), 0),
            "ms_per_message": round((delta.total_seconds()) * 1000 / self.count, 3),
        }


def prepare_work(opts):
    return Pub(
        host=opts.host,
        port=opts.port,
        topic_root=opts.topic_root,
        topic_id=opts.topic_id,
        type_name=opts.type_name,
        count=opts.count[0],
        qos=opts.qos,
        beats=opts.beats[0],
        beats_delay=opts.beats_delay[0],
        period=opts.period[0],
        telemetry_type=TelemetryType.from_str(opts.telemetry_type),
        datapoints=opts.datapoints,
    ).run


def cli_parser_run(parser: argparse.ArgumentParser):
    parser.add_argument("--host", default="127.0.0.1", help="MQTT broker host")
    parser.add_argument("--port", type=int, default=1883, help="MQTT broker port")
    parser.add_argument(
        "--iterations",
        type=int,
        default=1,
        help="Number of times/iterations to run the benchmark",
    )
    parser.add_argument(
        "--mqtt-device-topic-id",
        dest="topic_id",
        default="device/main//",
        help="The device MQTT topic identifier",
    )
    parser.add_argument(
        "--mqtt-topic-root", dest="topic_root", default="te", help="MQTT root prefix"
    )
    parser.add_argument(
        "--type_name",
        default="environment",
        dest="type_name",
        help="MQTT type name to use when publishing, e.g. environment",
    )
    parser.add_argument(
        "--clients",
        dest="clients",
        default=1,
        type=int,
        help="Number of concurrent MQTT clients which will publish the same amount of data",
    )

    parser.add_argument(
        "--qos",
        default=0,
        type=int,
        help="Quality of Service used to publish the MQTT messages",
    )
    parser.add_argument(
        "--datapoints",
        dest="datapoints",
        default=1,
        type=int,
        help="Number of datapoints to include in a single telemetry message",
    )

    parser.add_argument(
        "--count",
        default=[50],
        type=flag_range,
        help="Number of messages to send in one iteration (across multiple bursts)",
    )

    # Burst parameters
    parser.add_argument(
        "--period",
        default=[0],
        type=flag_range,
        help="Period in milliseconds between the start of the bursts. This will be ignored if the duration of the bursts is longer than the period",
    )
    parser.add_argument(
        "--beats",
        default=[1],
        type=flag_range,
        help="Number of beats/messages to publish in a burst",
    )
    parser.add_argument(
        "--beats-delay",
        default=[0],
        type=flag_range,
        dest="beats_delay",
        help="Delay in milliseconds between beats in a burst",
    )

    parser.add_argument(
        "--telemetry-type",
        default=str(TelemetryType.MEASUREMENT),
        type=str,
        dest="telemetry_type",
        choices=TelemetryType.values(),
        help="Telemetry data type to use when benchmarking",
    )

    parser.add_argument(
        "--restart-service",
        dest="restart_service",
        action="store_true",
        help="Restart tedge-mapper-c8y service after an iteration which dropped message were detected",
    )
    parser.add_argument(
        "--pretty", action="store_true", help="Pretty print the JSON results"
    )


def flag_range(value: str) -> List[int]:
    """Parse a flag range converting a range defined as a string to a list of integers (inclusive)

    The ranges can be provided using one of the following formats (note, <end> values are inclusive!)

    <start>:<end>
    <start>:<step>:<end>
    <value1>,<value2>,<value3>,<value4>,...

    Examples:

        0:1:5 -> [0, 1, 2, 3, 4, 5]

        10:5:20 -> [10, 15, 20]

        10,12,17,20 -> [10, 12, 18, 20]
    """
    if isinstance(value, list):
        return value

    # User provided a csv list of values to use instead of a range
    if "," in str(value):
        return [int(item) for item in value.split(",")]

    parts = str(value).split(":")
    if len(parts) == 3:
        output = list(range(int(parts[0]), int(parts[2]) + 1, int(parts[1])))
    elif len(parts) == 2:
        output = list(range(int(parts[0]), int(parts[1]) + 1))
    elif len(parts) == 1:
        output = [int(parts[0])]
    return output


def zip_values(items) -> List[any]:
    """Zip multiple uneven lists by repeating the value in any lists
    which are shorter than the longer list
    """
    var_index = 0
    max_len = 0
    for i, item in enumerate(items):
        if len(item) > max_len:
            max_len = len(item)
            var_index = i

    x_axis = []
    matrix = []
    for i, item in enumerate(items):
        if len(item) < max_len:
            matrix.append(cycle(item))
        else:
            matrix.append(item)

        if i == var_index:
            x_axis = list(item)

    return [list(zip(*matrix)), x_axis]


def run_benchmark(opts):
    params, x_axis = zip_values([opts.count, opts.beats, opts.beats_delay, opts.period])

    results = []

    iteration = 1

    while iteration <= opts.iterations:
        LOG.info(f"Running iteration: %d", iteration)
        for count, beats, beats_delay, period in params:
            setattr(opts, "count", [count])
            setattr(opts, "beats", [beats])
            setattr(opts, "beats_delay", [beats_delay])
            setattr(opts, "period", [period])

            LOG.info(
                "Starting benchmark: count=%d, beats=%d, beats_delay=%dms, period=%dms",
                count,
                beats,
                beats_delay,
                period,
            )
            m_log = multiprocessing.get_logger()
            m_log.setLevel(logging.DEBUG)
            pool = Pool(opts.clients)
            result = pool.map_async(prepare_work(opts), range(opts.clients))
            pool.close()
            pool.join()

            for item in result.get():
                results.append(item)

                if item["dropped_percent"] > 0:
                    if opts.restart_service:
                        LOG.warning(
                            "Detected dropped messages. Restarting tedge-mapper-c8y"
                        )
                        subprocess.check_call(
                            ["sudo", "systemctl", "restart", "tedge-mapper-c8y"]
                        )

                        # Wait for service to come up
                        time.sleep(5)
                    else:
                        LOG.warning("Detected dropped messages")

                # Pause before starting next test
                time.sleep(1)

        iteration += 1

    total_failed = len([item for item in results if item["dropped_percent"] > 0])
    success = total_failed == 0

    summary = {
        "ok": success,
        "iterations": len(results),
        "passed": len(results) - total_failed,
        "failed": total_failed,
        "results": results,
        # Store time series data in an array to make it easier to use in plots  (for future use)
        "x_axis": x_axis,
        "dropped_percent": [item["dropped_percent"] for item in results],
        "total": [item["total"] for item in results],
        "messages_per_second": [item["messages_per_second"] for item in results],
    }

    if opts.pretty:
        print(json.dumps(summary, indent=2))
    else:
        print(json.dumps(summary))
    return success


def configure():
    """Configure the mosquitto bridge settings to ignore specific messages to
    protect against publishing large volumes of messages to the cloud
    """
    bridge_config = Path("/etc/tedge/mosquitto-conf/c8y-bridge.conf")
    restart_required = False

    if not bridge_config.exists():
        LOG.info("%s file does not exist. Ignoring", str(bridge_config))
        return

    text = bridge_config.read_text("utf-8")

    if re.search("^topic measurement/measurements/create", text, re.MULTILINE):
        restart_required = True
        text = re.sub(
            "topic measurement/measurements/create",
            "#topic measurement/measurements/create",
            text,
            re.MULTILINE,
        )
        LOG.info("Modifying %s file", str(bridge_config))
        bridge_config.write_text(text, encoding="utf-8")

    if restart_required:
        LOG.info("Restarting mosquitto")
        if shutil.which("sudo"):
            subprocess.check_call(["sudo", "systemctl", "restart", "mosquitto"])
        else:
            subprocess.check_call(["systemctl", "restart", "mosquitto"])
        time.sleep(1)


def register_subcommand(parser: argparse.ArgumentParser):
    """Register a subcommand and command flags

    Store the name of the subcommand in the 'command' property of the parser which can be
    accessed to determine which subcommand was called by the user
    """
    parser.set_defaults(command=parser.prog.split(" ")[-1])
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Include verbose logging"
    )
    parser.add_argument("--debug", action="store_true", help="Include debug logging")
    return parser


def set_loglevel(logger: logging.Logger, level: Union[int, str]):
    """Set log level"""
    logger.setLevel(level)
    for handler in logger.handlers:
        if isinstance(handler, type(logging.StreamHandler())):
            handler.setLevel(level)


def main():
    """main"""
    parser = argparse.ArgumentParser(
        prog="benchmark",
        description="thin-edge.io benchmark script to validate the message throughput",
    )
    parser.set_defaults(command="")
    parser_subs = parser.add_subparsers()
    cli_parser_run(
        register_subcommand(
            parser_subs.add_parser(
                "run",
                help="Run benchmark",
                formatter_class=argparse.RawTextHelpFormatter,
                epilog=f"""

Examples:

  {sys.argv[0]} run --count 1000 --beats 100 --period 500
  # Run benchmarks by sending 1000 messages in bursts of 100 messages as quick as possible, and repeat every 500 milliseconds

  {sys.argv[0]} run --count 1000:500:2000 --beats 100 --period 500
  # Run multiple benchmarks increasing the amount of messages sent each time starting from 1000 messages to 2000 in increments of 500
                """,
            )
        )
    )
    register_subcommand(
        parser_subs.add_parser(
            "configure",
            help="Configure device in preparation for running the benchmark (e.g. update mosquitto bridge settings)",
        )
    )
    opts = parser.parse_args()

    # log level
    if getattr(opts, "debug", False):
        set_loglevel(LOG, logging.DEBUG)
    elif getattr(opts, "verbose", False):
        set_loglevel(LOG, logging.INFO)

    try:
        if not opts.command:
            parser.print_help()
            parser.exit(1)

        if opts.command == "configure":
            configure()
        elif opts.command == "run":
            configure()
            LOG.info("Running benchmark")
            success = run_benchmark(opts)
            LOG.info("Finished benchmark")

            if not success:
                parser.exit(1, "Benchmark failed")
    except Exception as ex:
        parser.exit(1, f"Benchmark failed. {ex}")

    parser.exit(0)


if __name__ == "__main__":
    main()
