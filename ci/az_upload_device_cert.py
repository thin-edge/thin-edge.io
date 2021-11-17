#!/usr/bin/python3
"""Upload a device certificate for Azure

See also:
https://docs.microsoft.com/en-us/rest/api/iothub/
https://docs.microsoft.com/en-us/rest/api/iothub/service/devices/create-or-update-identity


call example:
$ ./az_upload_device_cert.py -d devpi3 -t 01F...222 -u ThinEdgeHub -s iothubowner

Export environment variable SASKEYIOTHUB to the Shared access key of your IoT Hub.
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
    See also : https://docs.microsoft.com/en-us/legal/termsofuse
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

    try:
        sas_policy_primary_key_iothub = os.environ["SASKEYIOTHUB"]
    except KeyError:
        print("Error environment variable SASKEYIOTHUB not set")
        sys.exit(1)

    expiry = 3600

    uri = f"{hub}.azure-devices.net"

    # generate a shared access token
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
    elif req.status_code == 204:
        print("Unconditionally deleted the device")
        print("Deleted Device Properties: ", req.text)
    elif req.status_code == 404:
        print("Device is not there, not deleted")
    else:
        print(f"Error: {req.status_code}")
        print(f"Response Properties {req.text}")
        raise req.raise_for_status()


def upload_device_cert(devname, thprint, hub, sas_name, verbose):
    """Upload device certificate
    first generate an SAS access token, then upload the certificate"""

    try:
        sas_policy_primary_key_iothub = os.environ["SASKEYIOTHUB"]
    except KeyError:
        print("Error environment variable SASKEYIOTHUB not set")
        sys.exit(1)

    expiry = 3600

    uri = f"{hub}.azure-devices.net"

    # generate a sharec access token
    token = generate_sas_token(uri, sas_policy_primary_key_iothub, sas_name, expiry)

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

    try:
        os.environ["SASKEYIOTHUB"]
    except KeyError:
        print("Error environment variable SASKEYIOTHUB not set")

    devname = args.device
    thprint = args.thumbprint
    hub = args.hub
    sas_name = args.name
    verbose = args.verbose

    delete_device(devname, hub, sas_name)

    upload_device_cert(devname, thprint, hub, sas_name, verbose)


if __name__ == "__main__":
    main()
