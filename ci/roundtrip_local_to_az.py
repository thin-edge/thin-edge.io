#!/usr/bin/env python3

"""Perform a full roundtrip of messages from thin-edge to Azure IoT.

We publish with thin-edge to Azure IoT; then route the messages to a
Service Bus Queue; from there we retrieve the messages via a REST
Interface and compare them with what we have sent in the beginning.

Alternatively, we can use the Azure SDK to access the IoT Hub directly.

When this script is called you need to be already connected to Azure.

Call example:
./roundtrip_local_to_az.py  -a 10 -p sas_policy -b thinedgebus -q testqueue
    Set Env:
    - SASKEYQUEUE : Shared Access Key to the service bus queue

Alternatively:
./ci/roundtrip_local_to_az.py eventhub
    Set Env:
    - AZUREENDPOINT : Endpoint description string copied from the Azure UI
    - AZUREEVENTHUB : Name of the IoT Hub
"""

import argparse
import base64
import json
import json.decoder
import hashlib
import hmac
import os
import sys
import subprocess
import time
import urllib

import requests

import logging
from azure.eventhub import EventHubConsumerClient
import datetime

debug = False
if debug:
    logging.basicConfig(level=logging.INFO)
else:
    logging.basicConfig()

logger = logging.getLogger("roundtrip")
logger.setLevel(level=logging.INFO)


def publish_az(amount, topic, key):
    """Publish to Azure topic"""

    logger.info(f"Publishing messages to topic {topic}")

    for i in range(amount):
        message = f'{{"{key}": {i} }}'

        cmd = ["/usr/bin/tedge", "mqtt", "pub", topic, message]

        try:
            ret = subprocess.run(cmd, check=True)
        except subprocess.CalledProcessError as e:
            logger.error("Failed to publish %s", e)
            sys.exit(1)
        ret.check_returncode()

        logger.info("Published message: %s" % message)
        time.sleep(0.05)


def get_auth_token(sb_name, eh_name, sas_name, sas_value):
    """Create authentication token
    Analog to:
        https://docs.microsoft.com/en-us/rest/api/eventhub/generate-sas-token
    """
    newuri = urllib.parse.quote_plus(
        f"https://{sb_name}.servicebus.windows.net/{eh_name}"
    )
    sas_enc = sas_value.encode("utf-8")
    expiry = str(int(time.time()) + 10000)
    str_sign = newuri + "\n" + expiry
    signed_hmac = hmac.HMAC(sas_enc, str_sign.encode("utf-8"), hashlib.sha256)
    signature = urllib.parse.quote(base64.b64encode(signed_hmac.digest()))
    ret = {
        "sb_name": sb_name,
        "eh_name": eh_name,
        "token": f"SharedAccessSignature sr={newuri}&sig={signature}&se={expiry}&skn={sas_name}",
    }
    return ret


def retrieve_queue_az(
    sas_policy_name, service_bus_name, queue_name, amount, verbose, key
):
    """Get the published messages back from a service bus queue
    Probably soon obsolete.
    """

    try:
        sas_policy_primary_key = os.environ["SASKEYQUEUE"]
    except KeyError:
        print("Error environment variable SASKEYQUEUE not set")
        sys.exit(1)

    tokendict = get_auth_token(
        service_bus_name, queue_name, sas_policy_name, sas_policy_primary_key
    )

    token = tokendict["token"]

    if verbose:
        print("Token", token)

    # See also:
    # https://docs.microsoft.com/en-us/rest/api/servicebus/receive-and-delete-message-destructive-read

    url = (
        f"https://{service_bus_name}.servicebus.windows.net/{queue_name}/messages/head"
    )

    print(f"Downloading messages from {url}")
    headers = {
        "Accept": "application/json",
        "Content-Type": "application/json;charset=utf-8",
        "Authorization": token,
    }
    messages = []

    while True:

        try:
            req = requests.delete(url, headers=headers)
        except requests.exceptions.ConnectionError as e:
            print("Exception: ", e)
            print("Connection error: We wait for some seconds and then continue ...")
            time.sleep(10)
            continue

        if req.status_code == 200:
            text = req.text
            props = json.loads(req.headers["BrokerProperties"])
            number = props["SequenceNumber"]
            queuetime = props["EnqueuedTimeUtc"]

            try:
                data = json.loads(text)
                value = data[key]
            except json.decoder.JSONDecodeError:
                print("Json Parsing Error: ", text)
                value = None
            except KeyError:
                print("Parsing Error: ", text)
                value = None

            print(
                f'Got message {number} from {queuetime} message is "{text}" value: "{value}"'
            )
            messages.append(value)

        elif req.status_code == 204:
            print("Queue Empty:  HTTP status: ", req.status_code)
            break
        elif req.status_code == 401:
            print("Token Expired:  HTTP status: ", req.status_code)
            raise SystemError("Token Expired")
        else:
            print(req)
            print("Error HTTP status: ", req.status_code)
            raise SystemError("HTTP Error")

    if messages == list(range(amount)):
        print("Validation PASSED")
        return True
    else:
        print("Validation FAILED")
        return False


class EventHub:
    """Class to host all properties and access functions for an IoT Hub/ Eventhub
    Needs https://pypi.org/project/azure-eventhub

    Docs:
        https://docs.microsoft.com/en-us/azure/iot-hub/iot-hub-devguide-messages-read-builtin
        https://azuresdkdocs.blob.core.windows.net/$web/python/azure-eventhub/latest/azure.eventhub.html
        https://azuresdkdocs.blob.core.windows.net/$web/python/azure-eventhub/latest/azure.eventhub.html#azure.eventhub.EventData
    """

    def __init__(self, message_key, amount):

        try:
            connection_str = os.environ["AZUREENDPOINT"]
        except KeyError:
            logger.error("Error environment variable AZUREENDPOINT not set")
            sys.exit(1)

        try:
            eventhub_name = os.environ["AZUREEVENTHUB"]
        except KeyError:
            logger.error("Error environment variable AZUREEVENTHUB not set")
            sys.exit(1)

        self.message_key = message_key
        self.amount = amount
        consumer_group = "$Default"
        timeout = 10  # 10s : minimum timeout

        self.client = EventHubConsumerClient.from_connection_string(
            connection_str,
            consumer_group,
            eventhub_name=eventhub_name,
            idle_timeout=timeout,
        )

        self.received_messages = []

    def on_error(self, partition_context, event):
        logger.error(
            "Received Error from partition {}".format(partition_context.partition_id)
        )
        logger.error(f"Event: {event}")

    def on_event(self, partition_context, event):
        logger.debug(
            "Received event from partition {}".format(partition_context.partition_id)
        )
        logger.debug(f"Event: {event}")

        if event == None:
            logger.debug("Timeout: Exiting event loop ... ")
            self.client.close()
            return

        partition_context.update_checkpoint(event)

        jevent = event.body_as_json()

        message = jevent.get(self.message_key)
        if message != None:
            logger.info("Matched key: %s" % message)
            self.received_messages.append(message)
        else:
            logger.info("Not matched key: %s" % jevent)

    def read_from_hub(self, start):
        """Read data from the event hub

        Possible values for start:
        start = "-1" : Read all messages
        start = "@latest" : Read only the latest messages
        start = datetime.datetime.now(tz=datetime.timezone.utc) : use current sdate

        When no messages are received the client.receive will return.
        """

        with self.client:
            self.client.receive(
                on_event=self.on_event,
                on_error=self.on_error,
                starting_position=start,
                max_wait_time=10,
            )
            logger.info("Exiting event loop")

    def validate(self):
        """Validate the messages that we have received against"""

        if self.received_messages == list(range(self.amount)):
            print("Validation PASSED")
            return True
        else:
            print("Validation FAILED")
            return False


def main():
    """Main entry point"""
    parser = argparse.ArgumentParser()
    parser.add_argument("method", choices=["eventhub", "servicebus"])
    parser.add_argument("-b", "--bus", help="Service Bus Name")
    parser.add_argument("-p", "--policy", help="SAS Policy Name")
    parser.add_argument("-q", "--queue", help="Queue Name")
    parser.add_argument(
        "-a", "--amount", help="Amount of messages to send", type=int, default=20
    )
    parser.add_argument("-v", "--verbose", help="Verbosity", action="count", default=0)
    args = parser.parse_args()

    amount = args.amount
    sas_policy_name = args.policy
    service_bus_name = args.bus
    queue_name = args.queue
    verbose = args.verbose
    method = args.method

    if method == "servicebus":
        try:
            os.environ["SASKEYQUEUE"]
        except KeyError:
            print("Error environment variable SASKEYQUEUE not set")
            sys.exit(1)

    try:
        device = os.environ["C8YDEVICE"]
    except KeyError:
        print("Error environment variable C8YDEVICE not set")
        sys.exit(1)

    # Send roundtrip via the tedge mapper
    mqtt_topic = "tedge/measurements"
    # In case that we want to avoid the azure mapper
    # mqtt_topic = "az/messages/events/"

    message_key = "thin-edge-azure-roundtrip-" + device

    if method == "eventhub":

        eh = EventHub(message_key=message_key, amount=amount)

        start = datetime.datetime.now(tz=datetime.timezone.utc)

        publish_az(amount, mqtt_topic, message_key)

        eh.read_from_hub(start)
        if not eh.validate():
            sys.exit(1)

    elif method == "servicebus":

        publish_az(amount, mqtt_topic, message_key)

        result = retrieve_queue_az(
            sas_policy_name, service_bus_name, queue_name, amount, verbose, message_key
        )

        if not result:
            sys.exit(1)


if __name__ == "__main__":
    main()
