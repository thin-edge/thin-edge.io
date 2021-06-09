#!/usr/bin/python3

"""Perform a full roundtrip of messages from thin-edge to Azure IoT

We publish with thin-edge to Azure IoT; then route the messages to a
Service Bus Queue; from there we retrieve the messages via a REST
Interface and compare them with what we have sent in the beginning.

When this script is called you need to be already connected to Azure.

Call example:
$ ./roundtrip_local_to_az.py  -a 10 -p sas_policy -b thinedgebus -q testqueue
"""

import argparse
import base64
import json
import hashlib
import hmac
import os
import sys
import subprocess
import time
import urllib

import requests


def publish_az(amount):
    """Publish to Azure topic"""

    for i in range(amount):
        message = f'{{"cafe": {i} }}'
        cmd = ["/usr/bin/tedge", "mqtt", "pub", "az/messages/events/", message]
        subprocess.run(cmd)
        print("Published message: ", message)
        time.sleep(0.05)


# Function taken from :
# https://docs.microsoft.com/en-us/rest/api/eventhub/generate-sas-token
# TODO : Care about license for this part
def get_auth_token(sb_name, eh_name, sas_name, sas_value):
    """
    Returns an authorization token dictionary
    for making calls to Event Hubs REST API.
    """
    uri = urllib.parse.quote_plus(
        "https://{}.servicebus.windows.net/{}".format(sb_name, eh_name)
    )
    sas = sas_value.encode("utf-8")
    expiry = str(int(time.time() + 10000))
    string_to_sign = (uri + "\n" + expiry).encode("utf-8")
    signed_hmac_sha256 = hmac.HMAC(sas, string_to_sign, hashlib.sha256)
    signature = urllib.parse.quote(base64.b64encode(signed_hmac_sha256.digest()))
    return {
        "sb_name": sb_name,
        "eh_name": eh_name,
        "token": "SharedAccessSignature sr={}&sig={}&se={}&skn={}".format(
            uri, signature, expiry, sas_name
        ),
    }


def retrieve_queue_az(sas_policy_name, service_bus_name, queue_name, amount, verbose):
    """Get the published messages back from a service bus queue"""

    if "SASKEYQUEUE" in os.environ:
        sas_policy_primary_key = os.environ["SASKEYQUEUE"]
    else:
        print("Error environment variable SASKEYQUEUE not set")
        sys.exit(1)

    tokendict = get_auth_token(
        service_bus_name, queue_name, sas_policy_name, sas_policy_primary_key
    )

    token = tokendict["token"]

    if verbose:
        print("Token", token)

    # See also
    # https://docs.microsoft.com/en-us/rest/api/servicebus/receive-and-delete-message-destructive-read

    # Do it manuylly with curl:
    # curl --request DELETE \
    # --url "http{s}://thinedgebus.servicebus.windows.net/testqueue/messages/head" \
    # --header "Accept: application/json" \
    # --header "Content-Type: application/json;charset=utf-8" \
    # --header "Authorization: $SASTOKEN"     --verbose

    url = "https://thinedgebus.servicebus.windows.net/testqueue/messages/head"

    headers = {
        "Accept": "application/json",
        "Content-Type": "application/json;charset=utf-8",
        "Authorization": token,
    }
    messages = []

    while True:
        req = requests.delete(url, headers=headers)

        if req.status_code == 200:
            text = req.text
            props = json.loads(req.headers["BrokerProperties"])
            number = props["SequenceNumber"]
            time = props["EnqueuedTimeUtc"]

            try:
                data = json.loads(text)
                value = data["cafe"]
            except:
                print("Parsing Error", text)
                value = None

            print(f"Got message {number} from {time} message is {text} value: {value}")
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


if __name__ == "__main__":

    parser = argparse.ArgumentParser()
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

    publish_az(amount)
    result = retrieve_queue_az(
        sas_policy_name, service_bus_name, queue_name, amount, verbose
    )

    if not result:
        sys.exit(1)
