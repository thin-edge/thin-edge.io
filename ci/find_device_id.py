#!/usr/bin/python3

"""Find the device ID for a given device in C8y and print it on the
command line.

For example:

    python3 ci/find_device_id.py --tenant t493319102 --user octocat
      --device devraspberrypi --url thin-edge-io.eu-latest.cumulocity.com

"""

import argparse
import os
import sys
from c8y_api import CumulocityApi


def get_device_id(c8y, name):
    """retrive the current device ID"""
    for dev in c8y.device_inventory.get_all():
        if name == dev.name:
            return dev.id
    return None


def main():
    """Main entry point"""

    parser = argparse.ArgumentParser()
    parser.add_argument("--tenant", required=True, help="C8y Tenant")
    parser.add_argument("--user", required=True, help="C8y User")
    parser.add_argument("--device", required=True, help="Device to find")
    parser.add_argument("--url", required=True, help="URL of C8y")
    parser.add_argument("--verbose", "-v", action="count", default=0)

    args = parser.parse_args()

    tenant = args.tenant
    user = args.user
    device_name = args.device
    url = args.url
    verbose = args.verbose

    try:
        password = os.environ["C8YPASS"]
    except KeyError:
        print("Please export your password into $C8YPASS")
        sys.exit(1)

    c8y = CumulocityApi(url, tenant, user, password)

    device_id = get_device_id(c8y, device_name)
    if device_id:
        if verbose:
            print("The current device ID is:")
        print(device_id)

    else:
        print("Failed to find device in C8y")
        sys.exit(1)


if __name__ == "__main__":
    main()
