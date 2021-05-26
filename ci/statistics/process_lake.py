#!/usr/bin/python3
# source ~/env-bigquery/bin/activate
# export GOOGLE_APPLICATION_CREDENTIALS="/home/micha/Project-SAG/Statistics/sturdy-mechanic-312713-14b2e55c4ad0.json"

import argparse
import sys
import time
import logging
import json
import subprocess
import os
import os.path
from pathlib import Path
import numpy as np
from numpy.core.records import array

import databases as db

def unzip_results(lake):
    p = Path(lake)
    for child in p.iterdir():
        if child.is_dir():
            logging.debug(child.name)
            pass
        elif child.name.endswith(".zip"):
            logging.debug(child.name)
            new_name = child.name.removesuffix(".zip")
            new_folder = f"{new_name}_unpack/"
            if not os.path.exists(os.path.join(lake, new_folder)):
                proc = subprocess.run(["unzip", child.name, "-d", new_folder], cwd=lake)


def get_measurement_folders(lake: Path) -> list[Path]:
    path = Path(lake)
    pathlist = sorted(
        Path(lake).glob("*_unpack"),
        key=lambda _: int(_.name.split("_")[1].split(".")[0]),
    )
    pathnames = []
    for path in pathlist:
        pathnames.append(path.name)
    return pathnames


def get_relevant_measurement_folders(lake, testdata):

    if testdata:
        earliest_valid = "results_1_unpack"
    else:
        # last earliest valid test run is 'results_107_unpack'
        earliest_valid = "results_107_unpack"

    folders = get_measurement_folders(lake)
    relevant_folders = []
    valid = False
    for folder in folders:
        if folder == earliest_valid:
            valid = True
        if valid:
            relevant_folders.append(folder)

    processing_range = len(relevant_folders)
    if processing_range == 0:
        raise SystemError("No reports found")
    logging.info(relevant_folders[-processing_range])

    assert relevant_folders[-processing_range] == earliest_valid

    logging.info(f"Procesing Range {processing_range}")

    logging.info("Procesing Build Numbers:")
    for m in relevant_folders[-processing_range:]:
        logging.info(m.split("_")[1])
    logging.info("")

    return relevant_folders, processing_range


def generate(style, show, lake, testdata):

    client, dbo, integer, conn = db.get_database(style)

    logging.info("Unzip Results")
    unzip_results(lake)

    logging.info("Sumarize List")

    relevant_folders, processing_range = get_relevant_measurement_folders(
        lake, testdata
    )

    logging.info("Postprocessing")

    data_length = 60

    cpu_array = db.CpuHistory(
        "ci_cpu_measurement_tedge_mapper",
        lake,
        processing_range * data_length,
        data_length,
        client,
        testdata,
    )

    cpu_array_mosquitto = db.CpuHistory(
        "ci_cpu_measurement_mosquitto",
        lake,
        processing_range * data_length,
        data_length,
        client,
        testdata,
    )

    cpu_array_long = db.CpuHistory(
        "ci_cpu_measurement_tedge_mapper_long",
        lake,
        processing_range * data_length * 2,
        data_length,
        client,
        testdata,
    )

    cpu_array_long_mosquitto = db.CpuHistory(
        "ci_cpu_measurement_mosquitto_long",
        lake,
        processing_range * data_length * 2,
        data_length,
        client,
        testdata,
    )

    mem_array = db.MemoryHistory(lake, processing_range * data_length, data_length, client, testdata)

    cpu_hist_array = db.CpuHistoryStacked(data_length, client, testdata)

    measurements = db.MeasurementMetadata(processing_range, client, testdata, lake)

    cpu_array.postprocess(
        relevant_folders,
        "publish_sawmill_record_statistics",
        "stat_mapper_stdout",
        "tedge_mapper",
    )

    cpu_array_mosquitto.postprocess(
        relevant_folders,
        "publish_sawmill_record_statistics",
        "stat_mosquitto_stdout",
        "mosquitto",
    )

    cpu_array_long.postprocess(
        relevant_folders,
        "publish_sawmill_record_statistics_long",
        "stat_mapper_stdout",
        "tedge_mapper",
    )

    cpu_array_long_mosquitto.postprocess(
        relevant_folders,
        "publish_sawmill_record_statistics_long",
        "stat_mosquitto_stdout",
        "mosquitto",
    )

    mem_array.postprocess(relevant_folders,
        "publish_sawmill_record_statistics",
        "statm_mapper_stdout",
        "tedge_mapper")

    measurements.postprocess(relevant_folders)

    cpu_hist_array.postprocess(relevant_folders, data_length, cpu_array)

    if show:
        cpu_array.show()
        cpu_array_mosquitto.show()
        cpu_array_long.show()
        mem_array.show()
        cpu_hist_array.show()
        cpu_array_long_mosquitto.show()
        measurements.show()

    logging.info("Uploading")

    cpu_array.delete_table()
    cpu_array.update_table()

    cpu_array_mosquitto.delete_table()
    cpu_array_mosquitto.update_table()

    cpu_array_long.delete_table()
    cpu_array_long.update_table()

    mem_array.delete_table()
    mem_array.update_table()

    cpu_hist_array.delete_table()
    cpu_hist_array.update_table()

    cpu_array_long_mosquitto.delete_table()
    cpu_array_long_mosquitto.update_table()

    measurements.delete_table()
    measurements.update_table()

    logging.info("Done")


def main():
    logging.basicConfig(level=logging.INFO)

    parser = argparse.ArgumentParser()
    parser.add_argument("style", type=str, help="Database style: [none, google]")
    parser.add_argument(
        "-t",
        "--testdata",
        action="store_true",
        help="Use test data sets",
        required=False,
    )
    parser.add_argument(
        "-s",
        "--show",
        action="store_true",
        help="Show data with matplotlib",
        required=False,
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Verbose", required=False
    )
    args = parser.parse_args()

    testdata = args.testdata
    style = args.style
    show = args.show
    verbose = args.verbose

    assert style in ["google", "none"]  #'ms'

    if testdata:
        logging.info("Using test data lake")
        lake = os.path.expanduser("~/DataLakeTest")
    else:
        logging.info("Using real data lake")
        lake = os.path.expanduser("~/DataLake")

    if verbose:
        logging.info("Enabling verbose mode")
        logging.basicConfig(level=logging.DEBUG)

    generate(style, show, lake, testdata)


if __name__ == "__main__":
    main()
