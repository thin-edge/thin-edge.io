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
    mock.return_value = ["results_0_unpack", "results_1_unpack", "results_2_unpack", "results_4_unpack"]

    ret, valid = pl.get_relevant_measurement_folders(lake, testdata)

    assert ret == exp
    assert valid == 3

def test_get_relevant_measurement_folders_mocked_5(mocker):

    exp = ["results_1_unpack", "results_2_unpack",
    "results_4_unpack", "results_5_unpack"]
    lake = os.path.expanduser("~/DataLakeTest")
    testdata = True

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = ["results_0_unpack",
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

def test_scrap_measurement_metadata():
    lake = os.path.expanduser("~/DataLakeTest")
    name = "system_test_1_metadata.json"
    file = os.path.join(lake,name)

    ret, date, url, name, branch = db.scrap_measurement_metadata(file)

    assert ret == 1
    assert date == "2021-05-19T15:21:01Z"
    assert url == "https://github.com/abelikt/thin-edge.io/actions/runs/857323798"
    assert name == "system-test-workflow"
    assert branch == "continuous_integration"

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
    cpu_array = db.CpuHistory("name", lake,
        len(relevant_measurement_folders) * data_length, data_length, client, testmode
    )
    cpu_array_long = db.CpuHistory("name", lake,
        len(relevant_measurement_folders) * data_length *2, data_length, client, testmode
    )
    mem_array = db.MemoryHistory(
        len(relevant_measurement_folders) * data_length, client, testmode
    )
    cpu_hist_array = db.CpuHistoryStacked(data_length, client, testmode)

    pl.postprocess_vals(
        data_length,
        relevant_measurement_folders,
        cpu_array,
        cpu_array_long,
        mem_array,
        cpu_hist_array,
        lake
    )

    cpu_array.postprocess(relevant_measurement_folders, "publish_sawmill_record_statistics", "stat_mapper_stdout")

    # programmatically reproduce the data set
    data = []
    for i in range(len(relevant_measurement_folders) * data_length):
        if i < 20:
            k = (i + 10) // 10
        else:
            k = 4

        data.append([i, k, i % 10, i + 1, i + 2, 0, 0])

    exp = np.array(data, dtype=np.int32)

    extensive_check = False
    if extensive_check:
        print("Expect")
        print(exp)
        print("There")
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
    cpu_array = db.CpuHistory("name", lake,
        len(relevant_measurement_folders) * data_length, data_length, client, testmode
    )
    cpu_array_long = db.CpuHistory("name", lake,
        len(relevant_measurement_folders) * data_length *2, data_length, client, testmode
    )
    mem_array = db.MemoryHistory(
        len(relevant_measurement_folders) * data_length, client, testmode
    )
    cpu_hist_array = db.CpuHistoryStacked(data_length, client, testmode)

    pl.postprocess_vals(
        data_length,
        relevant_measurement_folders,
        cpu_array,
        cpu_array_long,
        mem_array,
        cpu_hist_array,
        lake
    )

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
    cpu_array = db.CpuHistory( "name", lake,
        len(relevant_measurement_folders) * data_length, data_length, client, testmode
    )
    cpu_array_long = db.CpuHistory( "name", lake,
        len(relevant_measurement_folders) * data_length *2, data_length, client, testmode
    )
    mem_array = db.MemoryHistory(
        len(relevant_measurement_folders) * data_length, client, testmode
    )
    cpu_hist_array = db.CpuHistoryStacked(data_length, client, testmode)

    cpu_array.postprocess(relevant_measurement_folders,
        "publish_sawmill_record_statistics", "stat_mapper_stdout")

    pl.postprocess_vals(
        data_length,
        relevant_measurement_folders,
        cpu_array,
        cpu_array_long,
        mem_array,
        cpu_hist_array,
        lake
    )

    # programmatically reproduce the data set
    data = []
    for i in range(10):
        if i < 20:
            k = (i + 10) // 10
        else:
            k = 4
        data.append(
            [
                i,
                21 + i,
                22 + i,
                11 + i,
                12 + i,
                i + 1,
                i + 2,
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
    client = None
    testmode = True
    metadata = db.MeasurementMetadata(
        len(folders), client, testmode, lake)

    metadata.postprocess(folders)

    exp= [
        ( 0, 1,
        "2021-05-19T15:21:01Z",
        "https://github.com/abelikt/thin-edge.io/actions/runs/857323798",
        "system-test-workflow",
        "continuous_integration"),
        ( 1, 2,
        "2021-05-19T15:21:02Z",
        "https://github.com/abelikt/thin-edge.io/actions/runs/857323798",
        "system-test-workflow",
        "continuous_integration"),
        ( 2, 4,
        "2021-05-19T15:21:04Z",
        "https://github.com/abelikt/thin-edge.io/actions/runs/857323798",
        "system-test-workflow",
        "continuous_integration")
    ]

    assert metadata.array == exp

def test_show_metadata(mocker):
    lake = os.path.expanduser("~/DataLakeTest")

    folders = [
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]
    client = 'google'
    testmode = True
    metadata = db.MeasurementMetadata(
        len(folders), client, testmode, lake)

    metadata.postprocess(folders)
    metadata.show()

def test_upload_table_metadata(mocker):
    """"""
    lake = None
    client = None
    testmode = None
    metadata = db.MeasurementMetadata(
        3, client, testmode, lake)

    metadata.json_data = {'nope':'nope'}
    metadata.job_config = None

    # With this we inject a mock chain
    # load_table_from_json is called, it returns a load_job
    # load_job.running() returns False
    load_mock = mocker.MagicMock( name='load_job')
    load_mock.running = mocker.MagicMock( name='running', return_value = False)
    load_mock.errors = False
    load_table_mock = mocker.MagicMock(name='load_table_from_json', return_value = load_mock)
    mocker.patch.object(metadata, "client")
    metadata.client.load_table_from_json = load_table_mock

    metadata.upload_table()

    metadata.client.load_table_from_json.assert_called_once()

class TestMemoryHistory:
    def test_update_table_creates_attributes(self, mocker):
        base = db.MemoryHistory( 3, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None


    def test_update_table_calls_upload(self, mocker):
        metadata = db.MemoryHistory( 3, None, None)
        mocker.patch.object(metadata, "upload_table")

        metadata.update_table()

        metadata.upload_table.assert_called_once()

class TestCpuHistoryStacked:
    def test_update_table_creates_attributes(self, mocker):
        base = db.CpuHistoryStacked( 3, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None
        assert base.database != None

class TestCpuHistory:
    def test_update_table_creates_attributes(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")
        base = db.CpuHistory("name", lake, 3, None, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None


    def test_update_table_calls_upload(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")
        base = db.CpuHistory("name", lake, 3, None, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        base.upload_table.assert_called_once()

    def test_postprocess(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")
        base = db.CpuHistory("name", lake, 3, None, None, None)
        mock = mocker.patch.object(base, "scrap_cpu_stats")
        folders = [
            "results_1_unpack",
            "results_2_unpack",
            "results_4_unpack",
        ]
        base.postprocess(folders, "publish_sawmill_record_statistics", "stat_mapper_stdout")

        #mock.assert_called_once()
        assert mock.call_count == 3
        #mock.assert_called_with


class TestCpuHistoryStacked:
    def test_stacked_update_table_calls_upload(self, mocker):
        base = db.CpuHistoryStacked( 3, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        base.upload_table.assert_called_once()

class TestMetadata:
    def test_update_table_calls_upload(self, mocker):
        base = db.MeasurementMetadata( 1, None, None, None)
        mocker.patch.object(base, "upload_table")
        base.array = [[1,2,3,4,5,6]]

        base.update_table()

        base.upload_table.assert_called_once()

    def test_update_table_creates_attributes(self, mocker):
        base = db.MeasurementMetadata( 1, None, None, None)
        mocker.patch.object(base, "upload_table")
        base.array = [[1,2,3,4,5,6]]

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None

def test_upload_table_errors(mocker):
    """"""
    lake = None
    client = None
    testmode = None
    metadata = db.MeasurementMetadata(
        3, client, testmode, lake)

    metadata.json_data = {'nope':'nope'}
    metadata.job_config = None

    # With this we inject a mock chain
    # load_table_from_json is called, it returns a load_job
    # load_job.running() returns False
    load_mock = mocker.MagicMock( name='load_job')
    load_mock.running = mocker.MagicMock( name='running', return_value = False)

    load_mock.errors = True
    load_mock.error_results = "Error results"

    load_table_mock = mocker.MagicMock(name='load_table_from_json', return_value = load_mock)
    mocker.patch.object(metadata, "client")
    metadata.client.load_table_from_json = load_table_mock

    with pytest.raises(SystemError):
        metadata.upload_table()

def test_upload_table_delayed(mocker):
    """"""
    lake = None
    client = None
    testmode = None
    metadata = db.MeasurementMetadata(
        3, client, testmode, lake)

    metadata.json_data = {'nope':'nope'}
    metadata.job_config = None

    mocker.patch('time.sleep')

    # With this we inject a mock chain
    # load_table_from_json is called, it returns a load_job
    # load_job.running() returns False
    load_mock = mocker.MagicMock( name='load_job')
    load_mock.running = mocker.MagicMock(
        name='running', side_effect=[True, True, True, False])

    load_mock.errors = False

    load_table_mock = mocker.MagicMock(name='load_table_from_json', return_value = load_mock)
    mocker.patch.object(metadata, "client")
    metadata.client.load_table_from_json = load_table_mock

    metadata.upload_table()
    assert load_mock.running.call_count == 4

def test_upload_metadata_b(mocker):
    """"""
    lake = os.path.expanduser("~/DataLakeTest")

    folders = [
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]
    client = 'google'
    testmode = True
    metadata = db.MeasurementMetadata(
        len(folders), client, testmode, lake)

    metadata.postprocess(folders)

    mocker.patch.object(metadata, "upload_table")

    metadata.update_table()

    metadata.upload_table.assert_called_once_with()




