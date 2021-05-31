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
    valid = "results_1_unpack"

    ret, valid = pl.get_relevant_measurement_folders(lake, valid)

    assert ret == exp
    assert valid == 3


def test_get_relevant_measurement_folders_mocked(mocker):

    exp = ["results_1_unpack", "results_2_unpack", "results_4_unpack"]
    lake = os.path.expanduser("~/DataLakeTest")
    valid = "results_1_unpack"

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = [
        "results_0_unpack",
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
    ]

    ret, valid = pl.get_relevant_measurement_folders(lake, valid)

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
    valid = "results_1_unpack"

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = [
        "results_0_unpack",
        "results_1_unpack",
        "results_2_unpack",
        "results_4_unpack",
        "results_5_unpack",
    ]

    ret, valid = pl.get_relevant_measurement_folders(lake, valid)

    assert ret == exp
    assert valid == 4


def test_get_relevant_measurement_folders_mocked_1(mocker):

    exp = ["results_1_unpack"]
    lake = os.path.expanduser("~/DataLakeTest")
    valid = "results_1_unpack"
    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = ["results_1_unpack"]

    ret, valid = pl.get_relevant_measurement_folders(lake, valid)

    assert ret == exp
    assert valid == 1


def test_get_relevant_measurement_folders_mocked_0(mocker):

    lake = os.path.expanduser("~/DataLakeTest")
    valid = "results_1_unpack"

    mock = mocker.patch("process_lake.get_measurement_folders")
    mock.return_value = ["results_0_unpack"]

    with pytest.raises(SystemError):
        pl.get_relevant_measurement_folders(lake, valid)


def test_generate(mocker):
    """Integrtion test for generate"""

    lake = os.path.expanduser("~/DataLakeTest")
    show = False
    style = "none"
    testdata = True
    foldermock = mocker.patch(
        "process_lake.get_relevant_measurement_folders", return_value=(3, 1)
    )
    earliest = "results_107_unpack"
    cputables = 4
    memtables = 1
    stacktables = 1
    metatables = 1

    cpumock = mocker.MagicMock(name="cpuobject")
    mocker.patch("databases.CpuHistory", return_value=cpumock)
    memmock = mocker.MagicMock(name="memobject")
    mocker.patch("databases.MemoryHistory", return_value=memmock)
    stackmock = mocker.MagicMock(name="memobject")
    mocker.patch("databases.CpuHistoryStacked", return_value=stackmock)
    metamock = mocker.MagicMock(name="memobject")
    mocker.patch("databases.MeasurementMetadata", return_value=metamock)

    pl.generate(style, show, lake, testdata, earliest)

    foldermock.assert_called_with(lake, earliest)
    cpumock.update_table.assert_called_with()
    assert cpumock.update_table.call_count == cputables
    memmock.update_table.assert_called_with()
    assert memmock.update_table.call_count == memtables
    stackmock.update_table.assert_called_with()
    assert stackmock.update_table.call_count == stacktables
    metamock.update_table.assert_called_with()
    assert metamock.update_table.call_count == metatables

    # we seem to not be able to use ANY here, so assert at least the
    # call count

    assert cpumock.postprocess.call_count == cputables
    assert memmock.postprocess.call_count == memtables
    assert stackmock.postprocess.call_count == stacktables
    assert metamock.postprocess.call_count == metatables


def test_postprocess_vals_cpu():
    """This is an integration test!
    Tightnen current functionality for now
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

        if i < 18 or i >= 20:
            ut = i + 1
            st = i + 2
        else:
            ut = 0  # hint: missing data here
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
    """This is an integration test!
    Tightnen current functionality for now
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
        client,
        testmode,
    )

    mem_array.postprocess(
        relevant_measurement_folders,
        "publish_sawmill_record_statistics",
        "statm_mapper_stdout",
        "tedge_mapper",
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
    """This is an integration test!
    Tightnen current functionality for now
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
        client,
        testmode,
    )

    cpu_hist_array.postprocess(
        relevant_measurement_folders,
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
                i,  # id
                21 + i,  # user time 4
                22 + i,  # system time  4
                ut2,  # user time 2
                st2,  # system time 2
                i + 1,  # user time 1
                i + 2,  # system time 1
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
    """This is an integration test!
    Tightnen current functionality for now
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
        lake, "ci_measurements", len(folders), data_length, client, testmode
    )

    metadata.postprocess(folders)

    exp = [
        (
            0,
            1,
            "2021-05-19T15:21:01Z",
            "https://github.com/abelikt/thin-edge.io/actions/runs/857323798",
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


class TestMain:
    lake = os.path.expanduser("~/DataLake")
    testlake = os.path.expanduser("~/DataLakeTest")
    earliest = "results_107_unpack"
    testearliest = "results_1_unpack"

    def test_main_with_testdata_nostyle_testdata(self, mocker):
        mocker.patch("sys.argv", ["procname", "-t", "none"])
        genmock = mocker.patch("process_lake.generate")
        pl.main()
        genmock.assert_called_with(
            "none", False, self.testlake, True, self.testearliest
        )

    def test_main_with_testdata_googlestyle_testdata(self, mocker):
        mocker.patch("sys.argv", ["procname", "-t", "google"])
        genmock = mocker.patch("process_lake.generate")
        pl.main()
        genmock.assert_called_with(
            "google", False, self.testlake, True, self.testearliest
        )

    def test_main_with_testdata_nostyle(self, mocker):
        mocker.patch("sys.argv", ["procname", "none"])
        genmock = mocker.patch("process_lake.generate")
        pl.main()
        genmock.assert_called_with("none", False, self.lake, False, self.earliest)

    def test_main_with_testdata_googlestyle(self, mocker):
        mocker.patch("sys.argv", ["procname", "google"])
        genmock = mocker.patch("process_lake.generate")
        pl.main()
        genmock.assert_called_with("google", False, self.lake, False, self.earliest)

    def test_main_with_testdata_googlestyle_show(self, mocker):
        mocker.patch("sys.argv", ["procname", "-s", "google"])
        genmock = mocker.patch("process_lake.generate")
        pl.main()
        genmock.assert_called_with("google", True, self.lake, False, self.earliest)

    def test_main_with_testdata_googlestyle_show_testdata(self, mocker):
        mocker.patch("sys.argv", ["procname", "-s", "-t", "google"])
        genmock = mocker.patch("process_lake.generate")
        pl.main()
        genmock.assert_called_with(
            "google", True, self.testlake, True, self.testearliest
        )

    def test_main_with_testdata_googlestyle_show_verbose(self, mocker):
        mocker.patch("sys.argv", ["procname", "-s", "-t", "-v", "google"])
        genmock = mocker.patch("process_lake.generate")
        pl.main()
        genmock.assert_called_with(
            "google", True, self.testlake, True, self.testearliest
        )

    def test_main_with_invalid_arg(self, mocker):
        mocker.patch("sys.argv", ["procname", "nope"])
        with pytest.raises(AssertionError):
            pl.main()
