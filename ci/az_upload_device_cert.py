#!/usr/bin/python3
"""Upload a device certificate for Azure

See also:
https://docs.microsoft.com/en-us/rest/api/iothub/
https://docs.microsoft.com/en-us/rest/api/iothub/service/devices/create-or-update-identity


call example:
$ ./az_upload_device_cert.py -d devpi3 -t 01F...222 -u ThinEdgeHub -s iothubowner
"""

import argparse
import base64
import hashlib
import hmac
import os
import sys
import time
import urllib

import requests


def generate_sas_token(uri, key, policy_name, expiry=3600):
    """Function copied from Microsoft documentation
    https://docs.microsoft.com/en-us/azure/iot-hub/iot-hub-dev-guide-sas?tabs=python
    TODO : Care about license for this part
    """
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


def delete_device(devname, hub, sas_name):
    """Delete the device"""

    if "SASKEYIOTHUB" in os.environ:
        sas_policy_primary_key_iothub = os.environ["SASKEYIOTHUB"]
    else:
        print("Error environment variable SASKEYIOTHUB not set")
        sys.exit(1)

    expiry = 3600

    uri = f"{hub}.azure-devices.net"

    # generate a sharec access token
    token = generate_sas_token(uri, sas_policy_primary_key_iothub, sas_name, expiry)

    url = f"https://{hub}.azure-devices.net/devices/{devname}"

    headers = {
        "Content-Type": "application/json",
        "Content-Encoding": "utf-8",
        "Authorization": token,
        "If-Match": "*",
    }

    params = {"api-version": "2020-05-31-preview"}

    req = requests.delete(url, params=params, headers=headers)

    if req.status_code == 200:
        print("Deleted the device")
        print("Device Properties: ", req.text)
    if req.status_code == 204:
        print("Unconditionally deleted the device")
        print("Deleted Device Properties: ", req.text)
    elif req.status_code == 404:
        print("Device is not there, not deleted")
    else:
        print(f"Error: {req.status_code}")
        print("Response Properties", req.text)
        sys.exit(1)


def upload_device_cert(devname, thprint, hub, sas_name, verbose):
    """Upload device certificate
    first generate an SAS access token, then upload the certificate"""

    if "SASKEYIOTHUB" in os.environ:
        sas_policy_primary_key_iothub = os.environ["SASKEYIOTHUB"]
    else:
        print("Error environment variable SASKEYIOTHUB not set")
        sys.exit(1)

    expiry = 3600

    uri = f"{hub}.azure-devices.net"

    # generate a sharec access token
    token = generate_sas_token(uri, sas_policy_primary_key_iothub, sas_name, expiry)

    # print(token)

    # Do it manually with curl:
    # curl -L -i -X PUT \
    # https://ThinEdgeHub.azure-devices.net/devices/devpi3?api-version=2020-05-31-preview \
    # -H 'Content-Type:application/json' -H 'Content-Encoding:utf-8' -H "Authorization:$KEY" \
    # -d '{"deviceId":"devpi3", "authentication": {"type" : "selfSigned","x509Thumbprint": \
    # { "primaryThumbprint":"01FDB2436885747A1174B1C95A1E884E8512E222" }}}'

    # Now upload the certificate

    url = f"https://{hub}.azure-devices.net/devices/{devname}"

    headers = {
        "Content-Type": "application/json",
        "Content-Encoding": "utf-8",
        "Authorization": token,
    }

    params = {"api-version": "2020-05-31-preview"}

    data = (
        '{"deviceId":"%s", "authentication": {"type" : "selfSigned",' % devname
        + '"x509Thumbprint": { "primaryThumbprint":"%s", "secondaryThumbprint":"%s" }}}'
        % (thprint, thprint)
    )

    req = requests.put(url, data, params=params, headers=headers)

    if req.status_code == 200:
        print("Uploaded device certificate")
        if verbose:
            print("Uploaded Device Properties : ", req.text)
    else:
        print(f"Error: {req.status_code}")
        print("Response Properties", req.text)


def main():
    """Main entry point"""
    parser = argparse.ArgumentParser()
    parser.add_argument("-d", "--device", help="Device name")
    parser.add_argument("-t", "--thumbprint", help="Device thumbprint")
    parser.add_argument("-u", "--hub", help="IoT Hub")
    parser.add_argument("-s", "--name", help="Name of the IoT hub SAS policy")

    parser.add_argument("-v", "--verbose", help="Verbosity", action="count", default=0)
    args = parser.parse_args()

    if not "SASKEYIOTHUB" in os.environ:
        print("Error environment variable SASKEYIOTHUB not set")
        sys.exit(1)

    devname = args.device
    thprint = args.thumbprint
    hub = args.hub
    sas_name = args.name
    verbose = args.verbose

    delete_device(devname, hub, sas_name)

    upload_device_cert(devname, thprint, hub, sas_name, verbose)


if __name__ == "__main__":
    main()
