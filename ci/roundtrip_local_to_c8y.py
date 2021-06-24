#!/usr/bin/python3

"""
This is a hack to do a full roundtrip of data from thin-edge to c8y and back.

It publishes a sequence of numbers and expects to read them back from the cloud.
For thin-edge JSON the sawtooth publisher is used to publish.
For SmartREST the tedge is used to publish.


Call example:
./roundtrip_local_to_c8y.py -m REST -pub ~/thin-edge.io/target/debug/examples/ -u <username> -t <tennant> -pass <pass> -id <id>
./roundtrip_local_to_c8y.py -m JSON -pub ~/thin-edge.io/target/debug/examples/ -u <username> -t <tennant> -pass <pass> -id <id>
"""

import argparse
import base64
from datetime import datetime, timedelta, timezone
import os
import sys
import time
from typing import Tuple
import requests

# Warning: the list begins with the earliest one
PAGE_SIZE = "500"

# sudo is currently needed to avoid "User's Home Directory not found."
CMD_PUBLISH_REST = "sudo tedge mqtt pub c8y/s/us 211,%i"

CMD_PUBLISH_JSON = "sawtooth_publisher %s %s 1 flux"


def is_timezone_aware(stamp):  #:datetime):
    """determine if object is timezone aware or naive
    See also: https://docs.python.org/3/library/datetime.html?highlight=tzinfo#determining-if-an-object-is-aware-or-naive
    """

    return stamp.tzinfo is not None and stamp.tzinfo.utcoffset(stamp) is not None


def act(path_publisher, mode, publish_amount, delay):
    """Act: Publishing values with temperature_publisher"""

    if mode == "JSON":
        print("Act: Publishing values with temperature_publisher")
        ret = os.system(
            os.path.join(path_publisher, CMD_PUBLISH_JSON % (delay, publish_amount))
        )
        if ret != 0:
            print("Error cannot run publisher")
            sys.exit(1)

    elif mode == "REST":
        print("Act: Publishing values with tedge pub")
        for i in range(0, int(publish_amount)):
            ret = os.system(CMD_PUBLISH_REST % i)
            if ret != 0:
                print("Error cannot run publisher")
                sys.exit(1)
            time.sleep(delay / 1000)
    else:
        sys.exit(1)

    print("Waiting 3s for values to arrive in C8y")

    time.sleep(3)


def retrieve_data(
    user, device_id, password, tenant, verbose, timeslot
) -> Tuple[requests.models.Response, datetime]:
    """Download via REST"""

    time_to = datetime.now(timezone.utc).replace(microsecond=0)
    time_from = time_to - timedelta(seconds=timeslot)

    assert is_timezone_aware(time_from)

    date_from = time_from.isoformat(sep="T")
    date_to = time_to.isoformat(sep="T")

    print(f"Gathering values from {time_from} to {time_to}")

    # TODO Add command line parameter: cloud = 'latest.stage.c8y.io'
    cloud = "eu-latest.cumulocity.com"

    url = f"https://{user}.{cloud}/measurement/measurements"
    payload = {
        "source": device_id,
        "pageSize": PAGE_SIZE,
        "dateFrom": date_from,
        "dateTo": date_to,
    }
    auth = bytes(f"{tenant}/{user}:{password}", "utf-8")
    header = {b"Authorization": b"Basic " + base64.b64encode(auth)}

    if verbose:
        print("URL: ", url)

    # TODO Add authorisation style as command line parameter
    # req = requests.get(url, auth=(user, password))

    req = requests.get(url, params=payload, headers=header)

    if verbose:
        print("Requested URL:", req.url)

    if req.status_code != 200:
        print("Http request failed !!!")
        sys.exit(1)

    return req, time_from


def check_timestamps(timestamps, laststamp):
    """Check if timestamps are in range and monotonically rinsing
    the timestamps that we read back are in UTC time.
    """

    failed = False

    warning = 0

    assert is_timezone_aware(laststamp)

    for tstamp in timestamps:

        if tstamp.endswith("Z"):
            # fromisoformat does not seem to cope with the Z, so we remove it
            # Workaround: remove the Z and make the timestamp UTC aware
            #
            # Alternativelyuse:
            # dateutil.parser.isoparse is available in the third-party package dateutil.
            # https://dateutil.readthedocs.io/en/stable/parser.html#dateutil.parser.isoparse

            tstamp = tstamp[:-1]
            tstamp += "+00:00"

        tstampiso = datetime.fromisoformat(tstamp)

        assert is_timezone_aware(tstampiso)

        if tstampiso > laststamp:
            laststamp = tstampiso
        elif tstampiso == laststamp:
            warning += 1
            print("Warning", tstampiso, "is equal to", laststamp)
            laststamp = tstampiso
        else:
            print("Oops", tstampiso, "is smaller than", laststamp)
            failed = True

    if warning:
        print(f"WARNING: found {warning} equal timestamps!")

    if not failed:
        print("Timestamp verification PASSED")
    else:
        print("Timestamp verification FAILED")
        return False

    return True


def assert_values(
    mode, user, device_id, password, tenant, verbose, publish_amount, timeslot
):
    """Assert: Retrieving data via REST interface"""

    print("Assert: Retrieving data via REST interface")

    req, time_from = retrieve_data(user, device_id, password, tenant, verbose, timeslot)

    response = req.json()

    assert "statistics" in response
    page_size = response["statistics"]["pageSize"]

    amount = len(req.json()["measurements"])

    print(f"Found {amount} recorded values: ")

    if page_size == amount:
        print(f"Got {amount} values: your page size {page_size} is probably to small!")
        sys.exit(1)

    values = []
    timestamps = []

    assert "measurements" in response

    for i in response["measurements"]:
        source = i["source"]["id"]
        assert source == device_id

        if mode == "JSON":

            assert i["type"] == "ThinEdgeMeasurement"

            try:
                value = i["Flux [F]"]["Flux [F]"]["value"]
            except KeyError:
                print(f"Error: Cannot parse response: {i}")
                sys.exit(1)
        elif mode == "REST":

            assert i["type"] == "c8y_TemperatureMeasurement"

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

    expected = list(range(0, int(publish_amount)))

    print("Retrieved values:")

    #del values[10] # to cause an error

    for v in range(len(values)):

        if v >= 1:
            if (values[v-1] + 1) != values[v]:
                print("error!")

        print(f"{values[v]:5} ", end="")
        if int(values[v]+1)%20 == 0: # use a new line when data is
            print("")



    print("")

    #print("Expected:", expected)
    print("Expected: ", expected[0], " ... ", expected[-1])

    if values == expected:
        print("Data verification PASSED")
    else:
        print("Data verification FAILED")
        sys.exit(1)

    ret = check_timestamps(timestamps, time_from)
    if not ret:
        sys.exit(1)


def main():
    """The 'main' function"""

    parser = argparse.ArgumentParser()
    parser.add_argument("-m", "--mode", help="Mode JSON or REST")
    parser.add_argument("-pub", "--publisher", help="Path to sawtooth_publisher")
    parser.add_argument("-u", "--user", help="C8y username")
    parser.add_argument("-t", "--tenant", help="C8y tenant")
    parser.add_argument("-pass", "--password", help="C8y Password")
    parser.add_argument("-id", "--id", help="Device ID for C8y")
    parser.add_argument("--verbose", "-v", action="count", default=0)
    parser.add_argument(
        "--size", "-s", type=int, help="Amount of values to publish", default=20
    )
    parser.add_argument("--slot", "-o", type=int, help="Timeslot size", default=10)
    parser.add_argument(
        "--delay", "-d", type=int, help="Delay between publishs", default=100
    )
    args = parser.parse_args()

    mode = args.mode
    assert mode in ("REST", "JSON")
    path_publisher = args.publisher
    verbose = args.verbose
    user = args.user
    tenant = args.tenant
    password = args.password
    device_id = args.id
    publish_amount = args.size
    timeslot = args.slot
    delay = args.delay

    if verbose:
        print(f"Mode: {mode}")
        print(f"Using path for publisher: {path_publisher}")
        print("Using user name: HIDDEN")
        print("Using tenant-id: HIDDEN")
        print("Using device-id: HIDDEN")

    act(path_publisher, mode, publish_amount, delay)

    assert_values(
        mode, user, device_id, password, tenant, verbose, publish_amount, timeslot
    )


if __name__ == "__main__":
    main()
