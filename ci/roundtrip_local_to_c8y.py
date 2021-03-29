#!/usr/bin/python3

"""
This is a hack to do a full roundtrip of data from thin-edge to c8y and back.

It publishes numbers from 0..19 and expects to read them back from the cloud.

Call example:
./roundtrip_local_to_c8y.py -m REST -pub ~/thin-edge.io/target/debug/examples/ -u <username> -t <tennant> -pass <pass> -id <id>
./roundtrip_local_to_c8y.py -m JSON -pub ~/thin-edge.io/target/debug/examples/ -u <username> -t <tennant> -pass <pass> -id <id>
"""

import argparse
import base64
from datetime import datetime, timedelta
import os
import sys
import time

import requests

# Warning: the list begins with the earliest one
PAGE_SIZE = "200"

# Seconds to retrieve from the past (smaller than 10 does not work all the time)
TIMESLOT = 10

CMD_PUBLISH_REST = "tedge mqtt pub c8y/s/us 211,%i"
CMD_PUBLISH_JSON = "sawtooth_publisher 100 20 1 flux"


def act(path_publisher, mode):
    """Act: Publishing values with temperature_publisher"""

    if mode == "JSON":
        print("Act: Publishing values with temperature_publisher")
        ret = os.system(os.path.join(path_publisher, CMD_PUBLISH_JSON))
        if ret != 0:
            print("Error cannot run publisher")
            sys.exit(1)

    elif mode == "REST":
        print("Act: Publishing values with tedge pub")
        for i in range(0, 20):
            ret = os.system(CMD_PUBLISH_REST % i)
            if ret != 0:
                print("Error cannot run publisher")
                sys.exit(1)
            time.sleep(0.1)
    else:
        sys.exit(1)

    print("Waiting 2s for values to arrive in C8y")

    time.sleep(2)


def retrieve_data(user, device_id, password, tenant, verbose):
    """Download via REST"""

    time_to = datetime.utcnow().replace(microsecond=0)
    time_from = time_to - timedelta(seconds=TIMESLOT)

    date_from = time_from.isoformat(sep="T") + 'Z'
    date_to = time_to.isoformat(sep="T") + 'Z'

    print(f"Gathering values from {time_from} to {time_to}")

    # example date format:
    # date_from = '2021-02-15T13:00:00Z'
    # date_to = '2021-02-15T14:00:00Z'

    # TODO Add command line parameter: cloud = 'latest.stage.c8y.io'
    cloud = "eu-latest.cumulocity.com"

    url = (
        f"https://{user}.{cloud}/measurement/measurements?"
        f"source={device_id}&pageSize={PAGE_SIZE}&"
        f"dateFrom={date_from}&dateTo={date_to}"
    )

    auth = bytes(f"{tenant}/{user}:{password}", "utf-8")

    header = {b"Authorization": b"Basic " + base64.b64encode(auth)}

    if verbose:
        print("URL: ", url)

    # TODO Add authorisation style as command line parameter
    # req = requests.get(url, auth=(user, password))

    req = requests.get(url, headers=header)

    if req.status_code != 200:
        print("Http request failed !!!")
        sys.exit(1)

    return req, time_from


def check_timestamps(timestamps, laststamp):
    """Check if timestamps are in range and monotonically rinsing
    the timestamps that we read back are in UTC time.
    """

    failed = False

    for tstamp in timestamps:
        # All timestamps seem to be in UTC time -> will end with 'Z'
        # fromisoformat does not seem to cope with the Z, so we remove it

        if tstamp.endswith("Z"):
            tstamp = tstamp[:-1]
        else:
            print("Timestamp verification: Z is missing")
            failed = True

        tstampiso = datetime.fromisoformat(tstamp)

        if tstampiso > laststamp:
            laststamp = tstampiso
        else:
            print("Oops", tstampiso, "is smaller than", laststamp)
            failed = True

    if not failed:
        print("Timestamp verification PASSED")
    else:
        print("Timestamp verification FAILED")
        sys.exit(1)


def assert_values(mode, user, device_id, password, tenant, verbose):
    """Assert: Retrieving data via REST interface"""

    print("Assert: Retrieving data via REST interface")

    req, time_from = retrieve_data(user, device_id, password, tenant, verbose)

    amount = len(req.json()["measurements"])

    print(f"Found {amount} recorded values: ")

    values = []
    timestamps = []

    for i in req.json()["measurements"]:

        if mode == "JSON":
            try:
                value = i["Flux [F]"]["Flux [F]"]["value"]
            except KeyError:
                print(f"Error: Cannot parse response: {i}")
                sys.exit(1)
        elif mode == "REST":
            try:
                value = i["c8y_TemperatureMeasurement"]["T"]["value"]
            except KeyError:
                print(f"Error: Cannot parse response: {i}")
                sys.exit(1)
        else:
            print(f"Error: Cannot parse response: {i}")
            sys.exit(1)

        tstamp = i["time"]
        if verbose:
            print("   ", tstamp, value)
        values.append(value)
        timestamps.append(tstamp)

    expected = list(range(0, 20))

    print("Retrieved:", values)
    print("Expected:", expected)

    if values == expected:
        print("Data verification PASSED")
    else:
        print("Data verification FAILED")
        sys.exit(1)

    check_timestamps(timestamps, time_from)


if __name__ == "__main__":

    parser = argparse.ArgumentParser()
    parser.add_argument("-m", "--mode", help="Mode JSON or REST")
    parser.add_argument("-pub", "--publisher", help="Path to sawtooth_publisher")
    parser.add_argument("-u", "--user", help="C8y username")
    parser.add_argument("-t", "--tenant", help="C8y tenant")
    parser.add_argument("-pass", "--password", help="C8y Password")
    parser.add_argument("-id", "--id", help="Device ID for C8y")
    parser.add_argument('--verbose', '-v', action='count', default=0)
    args = parser.parse_args()

    mode = args.mode
    assert mode in ("REST", "JSON")
    path_publisher = args.publisher
    verbose = args.verbose
    user = args.user
    tenant = args.tenant
    password = args.password
    device_id = args.id

    if verbose:
        print(f"Mode: {mode}")
        print(f"Using path for publisher: {path_publisher}")
        print(f"Using user name: HIDDEN")
        print(f"Using tenant-id: HIDDEN")
        print(f"Using device-id: HIDDEN")

    act(path_publisher, mode)

    assert_values(mode, user, device_id, password, tenant, verbose)
