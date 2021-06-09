#!/usr/bin/python3
"""Upload a device certificate for Azure"""

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

# Function taken from here
# https://docs.microsoft.com/en-us/azure/iot-hub/iot-hub-dev-guide-sas?tabs=python
# TODO : Care about license for this part
def generate_sas_token(uri, key, policy_name, expiry=3600):
    ttl = time.time() + expiry
    sign_key = "%s\n%d" % ((urllib.parse.quote_plus(uri)), int(ttl))
    # print(sign_key)
    signature = base64.b64encode(
        hmac.HMAC(
            base64.b64decode(key), sign_key.encode("utf-8"), hashlib.sha256
        ).digest()
    )

    rawtoken = {"sr": uri, "sig": signature, "se": str(int(ttl))}

    if policy_name is not None:
        rawtoken["skn"] = policy_name

    return "SharedAccessSignature " + urllib.parse.urlencode(rawtoken)


def upload_device_cert(devname, thprint, hub, sas_name):
    """Upload device certificate
    first generate an SAS access token, then upload the certificate"""

    if "SASKEYIOTHUB" in os.environ:
        sas_policy_primary_key_iothub = os.environ["SASKEYIOTHUB"]
    else:
        print("Error environment variable SASPOLICYKEY not set")
        sys.exit(1)

    expiry = 3600

    uri = f"{hub}.azure-devices.net"

    # generate a sharec acces token
    token = generate_sas_token(uri, sas_policy_primary_key_iothub, sas_name, expiry)

    # print(token)

    # Do it manually with curl:
    # curl -L -i -X PUT https://ThinEdgeHub.azure-devices.net/devices/devpi3?api-version=2020-05-31-preview \
    # -H 'Content-Type:application/json' -H 'Content-Encoding:utf-8' -H "Authorization:$KEY" \
    # -d '{"deviceId":"devpi3", "authentication": {"type" : "selfSigned","x509Thumbprint": \
    # { "primaryThumbprint":"01FDB2436885747A1174B1C95A1E884E8512E222" }}}'

    # Now upload the certificate

    url = f"https://ThinEdgeHub.azure-devices.net/devices/{devname}"

    headers = {
        "Content-Type": "application/json",
        "Content-Encoding": "utf-8",
        "Authorization": token,
    }

    params = {"api-version": "2020-05-31-preview"}

    data = (
        '{"deviceId":"%s", "authentication": {"type" : "selfSigned","x509Thumbprint": { "primaryThumbprint":%s }}}'
        % (devname, thprint)
    )

    req = requests.put(url, data, params=params, headers=headers)

    if req.status_code == 200:
        print("Uploaded device certificate")
        print("Device Properties", req.text)
    else:
        print(f"Error: {req.status_code}")
        print("Response Properties", req.text)


if __name__ == "__main__":

    # parser = argparse.ArgumentParser()
    # parser.add_argument("-b", "--bus", help="Service Bus Name")
    # parser.add_argument("-p", "--policy", help="SAS Policy Name")
    # parser.add_argument("-q", "--queue", help="Queue Name")
    # parser.add_argument(
    #    "-a", "--amount", help="Amount of messages to send", type=int, default=20
    # )
    # parser.add_argument("-v", "--verbose", help="Verbosity", action="count", default=0)
    # args = parser.parse_args()

    # amount = args.amount
    # sas_policy_name = args.policy
    # service_bus_name = args.bus

    devname = "devpi3"
    thprint = "01FDB2436885747A1174B1C95A1E884E8512E222"
    hub = "ThinEdgeHub"
    sas_name = "iothubowner"

    upload_device_cert(devname, thprint, hub, sas_name)
