# Hint install pytest in the google environment,
# otherwise it will watch for the google modules
# outside of the envionment.

# Workaround:
# python -m pytest test_process_lake.py

# https://pypi.org/project/pytest-mock/
# pip install pytest
# pip install pytest-mock
# pip install pytest-cov

import numpy as np
import os
from os.path import expanduser
from pathlib import Path
import pytest


import process_lake as pl
import databases as db


def test_get_measurement_foders():
    """Still clumsy check"""

    lake = os.path.expanduser("~/DataLakeTest")

    ret = pl.get_measurement_folders(lake)
    exp = [
        "results_0_unpack",
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]
    assert ret == exp


def test_get_relevant_measurement_folders_real():

    exp = ["results_1_unpack", "results_2_unpack", "results_4_unpack"]
    lake = os.path.expanduser("~/DataLakeTest")
    testdata = True

    ret, valid = pl.get_relevant_measurement_folders(lake, testdata)

    assert ret == exp
    assert valid == 3


def test_get_relevant_measurement_folders_mocked(mocker):

    exp = ["results_1_unpack", "results_2_unpack", "results_4_unpack"]
    lake = os.path.expanduser("~/DataLakeTest")
    testdata = True

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = [
        "results_0_unpack",
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]

    ret, valid = pl.get_relevant_measurement_folders(lake, testdata)

    assert ret == exp
    assert valid == 3


def test_get_relevant_measurement_folders_mocked_5(mocker):

    exp = [
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
        "results_5_unpack",
    ]
    lake = os.path.expanduser("~/DataLakeTest")
    testdata = True

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = [
        "results_0_unpack",
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
        "results_5_unpack",
    ]

    ret, valid = pl.get_relevant_measurement_folders(lake, testdata)

    assert ret == exp
    assert valid == 4


def test_get_relevant_measurement_folders_mocked_1(mocker):

    exp = ["results_1_unpack"]
    lake = os.path.expanduser("~/DataLakeTest")
    testdata = True

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = ["results_1_unpack"]

    ret, valid = pl.get_relevant_measurement_folders(lake, testdata)

    assert ret == exp
    assert valid == 1


def test_get_relevant_measurement_folders_mocked_0(mocker):

    exp = ["results_0_unpack"]
    lake = os.path.expanduser("~/DataLakeTest")
    testdata = True

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = ["results_0_unpack"]

    with pytest.raises(SystemError):
        pl.get_relevant_measurement_folders(lake, testdata)

def test_postprocess_vals_cpu():
    """Tightnen current functionality for now
    Probably too much for a simple test
    """
    lake = os.path.expanduser("~/DataLakeTest")
    relevant_measurement_folders = [
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]

    data_length = 10
    client = None
    testmode = True

    cpu_array = db.CpuHistory(
        lake,
        "name",
        len(relevant_measurement_folders),
        data_length,
        client,
        testmode,
    )

    cpu_array.postprocess(
        relevant_measurement_folders,
        "publish_sawmill_record_statistics",
        "stat_mapper_stdout",
        "tedge_mapper",
    )

    # programmatically reproduce the data set
    data = []
    for i in range(len(relevant_measurement_folders) * data_length):
        if i < 20:
            k = (i + 10) // 10
        else:
            k = 4

        if i < 18 or i>=20:
            ut = i + 1
            st = i + 2
        else:
            ut = 0 # hint: missing data here
            st = 0

        data.append([i, k, i % 10, ut, st, 0, 0])

    exp = np.array(data, dtype=np.int32)

    extensive_check = False
    if extensive_check:
        print("\nExpect")
        print(len(exp))
        print(exp)
        print("There")
        print(cpu_array.size)
        print(cpu_array.array)

        for i in range(len(data)):
            print("Line", i, np.array_equal(exp[i], cpu_array.array[i]))

    assert np.array_equal(exp, cpu_array.array)


def test_postprocess_vals_mem():
    """Tightnen current functionality for now
    Probably too much for a simple test
    """
    lake = os.path.expanduser("~/DataLakeTest")
    relevant_measurement_folders = [
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]
    data_length = 10
    client = None
    testmode = True

    mem_array = db.MemoryHistory(
        lake,
        "ci_mem_measurement_tedge_mapper",
        len(relevant_measurement_folders),
        data_length,
        client, testmode
    )

    mem_array.postprocess(relevant_measurement_folders,
        "publish_sawmill_record_statistics",
        "statm_mapper_stdout",
        "tedge_mapper")

    # programmatically reproduce the data set
    data = []
    for i in range(len(relevant_measurement_folders) * data_length):
        if i < 20:
            k = (i + 10) // 10
        else:
            k = 4
        data.append([i, k, i % 10, 100 + i, 200 + i, 300 + i, 400 + i, 500 + i])

    exp = np.array(data, dtype=np.int32)

    extensive_check = False
    if extensive_check:
        print("Expect")
        print(exp)
        print("There")
        print(mem_array.array)

        for i in range(len(data)):
            print("Line", i, np.array_equal(exp[i], mem_array.array[i]))

    assert np.array_equal(exp, mem_array.array)


def test_postprocess_vals_cpu_hist():
    """Tightnen current functionality for now
    Probably too much for a simple test
    """
    lake = os.path.expanduser("~/DataLakeTest")

    relevant_measurement_folders = [
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]
    data_length = 10
    client = None
    testmode = True

    cpu_array = db.CpuHistory(
        lake,
        "name",
        len(relevant_measurement_folders),
        data_length,
        client,
        testmode,
    )

    cpu_array.postprocess(
        relevant_measurement_folders,
        "publish_sawmill_record_statistics",
        "stat_mapper_stdout",
        "tedge_mapper",
    )

    cpu_hist_array = db.CpuHistoryStacked(
        lake,
        "ci_cpu_hist",
        len(relevant_measurement_folders),
        data_length,
        client, testmode
    )

    cpu_hist_array.postprocess(
        relevant_measurement_folders,
        data_length,
        cpu_array,
    )

    # programmatically reproduce the data set
    data = []
    for i in range(10):
        if i < 8:
            ut2 = 11 + i
            st2 = 12 + i
        else:
            ut2 = 0
            st2 = 0

        data.append(
            [
                i, #id
                21 + i, # user time 4
                22 + i, # system time  4
                ut2, # user time 2
                st2, # system time 2
                i + 1,  # user time 1
                i + 2, # system time 1
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]
        )

    exp = np.array(data, dtype=np.int32)

    extensive_check = False
    if extensive_check:
        print("Expect")
        print(exp)
        print("There")
        print(cpu_hist_array.array)

        for i in range(len(data)):
            print("Line", i, np.array_equal(exp[i], cpu_hist_array.array[i]))

    assert np.array_equal(exp, cpu_hist_array.array)


def test_postprocess_vals_metadata():
    """Tightnen current functionality for now
    Probably too much for a simple test
    """
    lake = os.path.expanduser("~/DataLakeTest")

    folders = [
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]
    data_length = 10
    client = None
    testmode = True


    metadata = db.MeasurementMetadata(
        lake,
        "ci_measurements",
        len(folders),
        data_length,
        client,
        testmode
        )

    metadata.postprocess(folders)

    exp = [
        (
            0,
            1,
            "2021-05-19T15:21:01Z",            "https://github.com/abelikt/thin-edge.io/actions/runs/857323798",
            "system-test-workflow",
            "continuous_integration",
        ),
        (
            1,
            2,
            "2021-05-19T15:21:02Z",
            "https://github.com/abelikt/thin-edge.io/actions/runs/857323798",
            "system-test-workflow",
            "continuous_integration",
        ),
        (
            2,
            4,
            "2021-05-19T15:21:04Z",
            "https://github.com/abelikt/thin-edge.io/actions/runs/857323798",
            "system-test-workflow",
            "continuous_integration",
        ),
    ]

    assert metadata.array == exp
