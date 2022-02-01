#!/bin/env python3

"""
Delete Device from Cumulocity

The device name will be read from the C8YDEVICE environment variable
The C8y password will be read from the C8YPASS environment variable

To run:

   python3 -m venv env-c8y-api
   source ~/pyenvs/c8y-api-env/bin/activate
   pip install c8y-api
   python3 delete_current_device.py
"""

import os
import sys
from c8y_api import CumulocityApi


def delete_object(c8y, obj):
    try:
        c8y.inventory.get(obj).delete()
    except KeyError:
        print(f"Object with {obj} not there")
    print(f"Deleted object with ID {obj}")


def delete_device(c8y, name):
    for dev in c8y.device_inventory.get_all():
        # print(
        #    "Trying device name:",
        #    dev.name,
        #    dev.id,
        # )
        if name == dev.name:
            print(f"Device has ID {dev.id}")
            delete_object(c8y, dev.id)
            return True
    return False


def main():
    try:
        device_name = os.environ["C8YDEVICE"]
    except KeyError:
        print("Please export your password into $DEVICE")
        sys.exit(1)
    try:
        password = os.environ["C8YPASS"]
    except KeyError:
        print("Please export your password into $C8YPASS")
        sys.exit(1)
    c8y = CumulocityApi(
        "https://thin-edge-io.eu-latest.cumulocity.com",
        "t493319102",
        "octocat",
        password,
    )
    if delete_device(c8y, device_name):
        print("Deleted device from C8y")
    else:
        print("Failed to delete device from C8y")
        sys.exit(1)


if __name__ == "__main__":
    main()
