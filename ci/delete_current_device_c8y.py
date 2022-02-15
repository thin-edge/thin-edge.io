#!/usr/bin/python3

"""
Delete device from Cumulocity with a given name.

For example:

    python3 ci/delete_current_device.py --tenant t493319102 --user octocat
      --device devraspberrypi --url thin-edge-io.eu-latest.cumulocity.com

"""

import argparse
import os
import sys
from c8y_api import CumulocityApi


def delete_object(c8y, obj):
    """Delete object from inventory"""
    try:
        c8y.inventory.get(obj).delete()
    except KeyError:
        print(f"Object with {obj} not there")
    print(f"Deleted object with ID {obj}")


def delete_device(c8y, name, verbose):
    """Delete the current device"""
    devices = c8y.device_inventory.get_all(name=name)
    if len(devices) == 1:
        dev = devices[0]
        print(f"Device has ID {dev.id}")
        delete_object(c8y, dev.id)
        return True
    return False


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

    if delete_device(c8y, device_name, verbose):
        print("Deleted device from C8y")
    else:
        print("Failed to delete device from C8y")
        sys.exit(1)


if __name__ == "__main__":
    main()
