#!/usr/bin/python3
"""Find the device ID for a given device in C8y

./find_device_id.py --tenant t493319102 --user octocat
 --device devraspberrypi --url thin-edge-io.eu-latest.cumulocity.com

TODO Combine this with the Cumulocity class that we have in the PySys Folder

"""


import argparse
import json
import os
import sys
import base64

import requests


def get_header(tenant, user, password):
    """Return an authorzation header"""
    auth = bytes(f"{tenant}/{user}:{password}", "utf-8")
    return {b"Authorization": b"Basic " + base64.b64encode(auth)}


def get_id(header, device, url, verbose):
    """Loop through devices to find id of device and return it"""
    page = 1  # same as 0
    url = f"https://{url}/inventory/managedObjects/"

    while True:
        params = {
            "fragmentType": "c8y_IsDevice",
            "pageSize": 1,
            "currentPage": page,
        }

        req = requests.get(url, params=params, headers=header, timeout=1)
        req.raise_for_status()

        objects = json.loads(req.text)

        if len(objects["managedObjects"]) == 0:
            # End of the list
            break

        device_c8y = objects["managedObjects"][0]["name"]
        device_url = objects["managedObjects"][0]["self"]
        device_id = objects["managedObjects"][0]["id"]

        if verbose:
            print(f'Found device"{device_c8y}" with id {device_id}')
            print("Device url", device_url)

        if device == device_c8y:
            if verbose:
                print(f'*** Device is "{device}"')
                print("*** Found Device at", device_url)
                print("*** Found Device wit ID", device_id)
            return device_id
        # Select next page
        page += 1
    return None


def main():
    """main entry point"""

    parser = argparse.ArgumentParser()
    parser.add_argument("--tenant", required=True, help="C8y Tenant")
    parser.add_argument("--user", required=True, help="C8y User")
    parser.add_argument("--device", required=True, help="Device to find")
    parser.add_argument("--url", required=True, help="URL of C8y")
    parser.add_argument("--verbose", "-v", action="count", default=0)

    args = parser.parse_args()

    tenant = args.tenant
    user = args.user
    device = args.device
    url = args.url
    verbose = args.verbose

    try:
        password = os.environ["C8YPASS"]
    except KeyError:
        print("Error environment variable C8YPASS not set")
        sys.exit(1)

    header = get_header(tenant, user, password)
    runid = get_id(header, device, url, verbose)

    if runid is None:
        print(f"Cannot find device with name {device}")
        sys.exit(1)

    if verbose == 0:
        print(runid)
    else:
        print(f"Found Id :{runid}")


if __name__ == "__main__":
    main()
