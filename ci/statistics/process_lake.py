# source ~/env-bigquery/bin/activate
# export GOOGLE_APPLICATION_CREDENTIALS="/home/micha/Project-SAG/Statistics/sturdy-mechanic-312713-14b2e55c4ad0.json"

# WT
# Error {'reason': 'quotaExceeded', 'location': 'max_dml_outstanding_per_table', 'message': 'Quota exceeded: Your table exceeded quota for total number of dml jobs writing to a table, pending + running. For more information, see https://cloud.google.com/bigquery/troubleshooting-errors'}


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

testdata = True
if testdata:
    lake = os.path.expanduser("~/DataLakeTest")
else:
    lake = os.path.expanduser("~/DataLake")

style = "google"  #'ms', 'google', 'none'

client, dbo, integer, conn = db.get_database(style)

logging.basicConfig(level=logging.INFO)


def scrap_mem(data_length, thefile, mesaurement_index, client, dbo, memidx, arr):
    with open(thefile) as thestats:
        lines = thestats.readlines()
        sample = 0
        for line in lines:
            # print(line)
            entries = line.split()
            # print(entries)
            size = entries[1 - 1]  #     (1) total program size
            resident = entries[2 - 1]  #   (2) resident set size
            shared = entries[3 - 1]  #     (3) number of resident shared pages
            text = entries[4 - 1]  #       (4) text (code)
            # lib = entries[5-1] #      (5) library (unused since Linux 2.6; always 0)
            data = entries[6 - 1]  #      (6) data + stack

            arr.insert_line(
                idx=memidx,
                mid=mesaurement_index,
                sample=sample,
                size=size,
                resident=resident,
                shared=shared,
                text=text,
                data=data,
            )
            sample += 1
            memidx += 1

    logging.debug(f"Read {sample} Memory stats")
    missing = data_length - sample
    for m in range(missing):

        arr.insert_line(
            idx=memidx,
            mid=mesaurement_index,
            sample=sample,
            size=0,
            resident=0,
            shared=0,
            text=0,
            data=0,
        )
        sample += 1
        memidx += 1

    return memidx


def scrap_cpu(data_length, thefile, mesaurement_index, client, dbo, cpuidx, arr):

    with open(thefile) as thestats:
        lines = thestats.readlines()
        sample = 0

        for line in lines:
            # print(line)
            entries = line.split()
            # print(len(entries))
            if len(entries) == 52 and entries[1] == "(tedge_mapper)":
                ut = int(entries[14 - 1])
                st = int(entries[15 - 1])
                ct = int(entries[16 - 1])
                cs = int(entries[17 - 1])
                # print(idx, ut,st,ct,cs)

                arr.insert_line(
                    idx=cpuidx,
                    mid=mesaurement_index,
                    sample=sample,
                    utime=ut,
                    stime=st,
                    cutime=ct,
                    cstime=cs,
                )
                sample += 1
                cpuidx += 1

    logging.debug(f"Read {sample} cpu stats")
    missing = data_length - sample
    for m in range(missing):
        arr.insert_line(
            idx=cpuidx,
            mid=mesaurement_index,
            sample=sample,
            utime=0,
            stime=0,
            cutime=0,
            cstime=0,
        )
        sample += 1
        cpuidx += 1

    return cpuidx


def postprocess_vals(
    data_length,
    measurement_folders,
    cpu_array,
    mem_array,
    cpu_hist_array,
):

    # overall row index for the cpu table
    cpuidx = 0
    # overall row index for the memory table
    memidx = 0

    for folder in measurement_folders:
        mesaurement_index = int(folder.split("_")[1].split(".")[0])

        statsfile = f"{lake}/{folder}/PySys/publish_sawmill_record_statistics/Output/linux/stat_mapper_stdout.out"
        cpuidx = scrap_cpu(
            data_length, statsfile, mesaurement_index, client, dbo, cpuidx, cpu_array
        )

        statsfile = f"{lake}/{folder}/PySys/publish_sawmill_record_statistics/Output/linux/statm_mapper_stdout.out"
        memidx = scrap_mem(
            data_length, statsfile, mesaurement_index, client, dbo, memidx, mem_array
        )

    #    for folder in measurement_folders[-10:]:
    mlen = len(measurement_folders)

    for i in range(data_length):
        cpu_hist_array.array[i, 0] = i

    processing_range = min(len(measurement_folders), 10)
    column = 1
    for m in range(mlen - 1, mlen - processing_range - 1, -1):
        # print(m)
        for i in range(data_length):
            # print( cpu_array.array[ m*60+i ,3],  cpu_array.array[ m*60+i ,4] )
            cpu_hist_array.array[i, column] = cpu_array.array[m * data_length + i, 3]
            cpu_hist_array.array[i, column + 1] = cpu_array.array[
                m * data_length + i, 4
            ]
        column += 2

    # print( cpu_hist_array.array )


def unzip_results():
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


def get_measurement_folders(path: str) -> list[Path]:
    pathlist = sorted(
        Path(lake).glob("*_unpack"),
        key=lambda _: int(_.name.split("_")[1].split(".")[0]),
    )
    pathnames = []
    for path in pathlist:
        pathnames.append(path.name)
    return pathnames


def get_relevant_measurement_folders():

    if testdata:
        earliest_valid = "results_1_unpack"
        processing_range = 3  # newest one 145
    else:
        # last earliest valid is 'results_107_unpack'
        processing_range = 25  # newest one 185
        earliest_valid = "results_107_unpack"

    relevant_folders = get_measurement_folders(Path(lake))[-processing_range:]

    print(relevant_folders[-processing_range])

    assert relevant_folders[-processing_range] == earliest_valid

    logging.info("Procesing Range " + str(len(relevant_folders[-processing_range:])))

    logging.info("Procesing Build Numbers:")
    for m in relevant_folders[-processing_range:]:
        print(m.split("_")[1], end=" ")
    print("")

    return relevant_folders, processing_range


def generate():

    logging.info("Unzip Results")
    unzip_results()

    logging.info("Sumarize List")

    relevant_folders, processing_range = get_relevant_measurement_folders()

    logging.info("Postprocessing")

    data_length = 60
    cpu_array = db.CpuHistory(processing_range * data_length, client, testdata)
    mem_array = db.MemoryHistory(processing_range * data_length, client, testdata)
    cpu_hist_array = db.CpuHistoryStacked(data_length, client, testdata)

    postprocess_vals(
        data_length,
        relevant_folders,
        cpu_array,
        mem_array,
        cpu_hist_array,
    )

    cpu_array.show()
    mem_array.show()
    cpu_hist_array.show()

    # sys.exit(1)

    logging.info("Uploading")

    cpu_array.delete_table()
    cpu_array.update_table()

    mem_array.delete_table()
    mem_array.update_table()

    cpu_hist_array.delete_table()
    cpu_hist_array.update_table()

    logging.info("Done")


if __name__ == "__main__":
    generate()
