import os
import pytest

import databases as db
from databases import MeasurementBase

# TODO mocker.ANY does not seem to work


class TestGetDatabase:
    def test_big_query(self, mocker):
        mocker.patch("google.cloud.bigquery.Client")
        # assert db.get_database("google") ==  (mocker.ANY, 'ADataSet', 'INT64', None)
        client, dbo, integer, conn = db.get_database("google")
        assert dbo == "ADataSet"
        assert integer == "INT64"
        assert conn == None

    def test_none(self, mocker):
        client, dbo, integer, conn = db.get_database("none")
        assert dbo == "Nopedb"
        assert integer == "Nopeint"
        assert conn == None

    def disabled_test_ms(self, mocker):
        """TODO: remove the import for that"""
        # mocker.patch("pymssql")
        client, dbo, integer, conn = db.get_database("ms")
        assert dbo == "dbo"
        assert integer == "INTEGER"
        assert conn != None


class TestMeasurementBase:
    def test_foldername_to_index(self):
        assert MeasurementBase.foldername_to_index("results_44_unpack") == 44
        assert MeasurementBase.foldername_to_index("results_2_unpack") == 2
        assert MeasurementBase.foldername_to_index("results_02_unpack") == 2

        with pytest.raises(SystemError):
            MeasurementBase.foldername_to_index("stuff_2_unpack")

        with pytest.raises(SystemError):
            MeasurementBase.foldername_to_index("results_2_unpac")


class TestMemoryHistory:
    def test_update_table_creates_attributes(self, mocker):
        base = db.MemoryHistory(None, "name", 3, 10, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None

    def test_update_table_calls_upload(self, mocker):
        base = db.MemoryHistory(None, "name", 3, 10, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        base.upload_table.assert_called_once()

    def disable_test_scrap_memory(self):
        """Todo: Write nice tests here"""
        base = db.MemoryHistory(None, 3, None, None, None)
        ret = base.scrap_mem()
        assert ret == 88

    def test_postrocess(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")

        base = db.MemoryHistory(lake, "name", 3, 10, None, None)
        mock = mocker.patch.object(base, "scrap_data")

        folders = [
            "results_1_unpack",
            "results_2_unpack",
            "results_4_unpack",
        ]

        exp = "{}/{}/PySys/name/Output/linux/filename.out"

        calls = [
            mocker.call(exp.format(lake, folders[0]), 1, base),
            mocker.call(exp.format(lake, folders[1]), 2, base),
            mocker.call(exp.format(lake, folders[2]), 4, base),
        ]

        base.postprocess(folders, "name", "filename", "binary")

        # mock.assert_called_once_with()

        assert mock.call_count == 3

        mock.assert_has_calls(calls)

    def test_upate_table(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")

        base = db.MemoryHistory(lake, "name", 3, 10, None, None)
        umock = mocker.patch.object(base, "upload_table")
        dmock = mocker.patch.object(base, "delete_table")

        base.update_table()

        umock.assert_called_once()
        dmock.assert_called_once()


class TestCpuHistory:
    def test_update_table_creates_attributes(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")
        base = db.CpuHistory(lake, "name", 3, 10, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None

    def test_update_table_calls_upload(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")
        base = db.CpuHistory(lake, "name", 3, 10, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        base.upload_table.assert_called_once()

    def test_postprocess(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")
        base = db.CpuHistory(lake, "name", 3, 10, None, None)
        mock = mocker.patch.object(base, "scrap_data")
        folders = [
            "results_1_unpack",
            "results_2_unpack",
            "results_4_unpack",
        ]
        base.postprocess(
            folders,
            "publish_sawmill_record_statistics",
            "stat_mapper_stdout",
            "tedge_mapper",
        )

        # mock.assert_called_once()
        assert mock.call_count == 3
        # mock.assert_called_with


class TestCpuHistoryStacked:
    def test_stacked_update_table_calls_upload(self, mocker):
        base = db.CpuHistoryStacked(None, "name", 3, 10, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        base.upload_table.assert_called_once()

    def test_update_table_creates_attributes(self, mocker):
        base = db.CpuHistoryStacked(None, "name", 3, 10, None, None)
        mocker.patch.object(base, "upload_table")

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None
        assert base.database != None

    def test_postprocess(self):
        """TODO: update me"""
        lake = os.path.expanduser("~/DataLakeTest")

        base = db.CpuHistoryStacked(None, "name", 3, 10, None, None)
        folders = [
            "results_1_unpack",
            "results_2_unpack",
            "results_4_unpack",
        ]

        cpu = db.CpuHistory(lake, "name", 3, 10, None, None)

        base.postprocess(folders, cpu)
        # cpu_array,
        # cpu_hist_array)


class TestMetadata:
    def test_update_table_calls_upload(self, mocker):
        base = db.MeasurementMetadata(None, "ci_measurements", 1, 10, None, None)
        umock = mocker.patch.object(base, "upload_table")
        dmock = mocker.patch.object(base, "delete_table")
        base.array = [[1, 2, 3, 4, 5, 6]]

        base.update_table()
        umock.assert_called_once()
        dmock.assert_called_once()

    def test_update_table_creates_attributes(self, mocker):
        base = db.MeasurementMetadata(None, "ci_measurements", 1, 10, None, None)
        mocker.patch.object(base, "upload_table")
        base.array = [[1, 2, 3, 4, 5, 6]]

        base.update_table()

        assert base.job_config != None
        assert base.json_data != None

    def test_show_metadata(self, mocker):
        lake = os.path.expanduser("~/DataLakeTest")

        folders = [
            "results_1_unpack",
            "results_2_unpack",
            "results_4_unpack",
        ]
        client = "google"
        testmode = True
        metadata = db.MeasurementMetadata(
            lake, "ci_measurements", 3, 10, client, testmode
        )
        metadata.postprocess(folders)
        metadata.show()

    def test_upload_table_metadata(self, mocker):
        """"""
        lake = None
        client = None
        testmode = None
        metadata = db.MeasurementMetadata(
            lake, "ci_measurements", 3, 10, client, testmode
        )
        metadata.json_data = {"nope": "nope"}
        metadata.job_config = None

        # With this we inject a mock chain
        # load_table_from_json is called, it returns a load_job
        # load_job.running() returns False
        load_mock = mocker.MagicMock(name="load_job")
        load_mock.running = mocker.MagicMock(name="running", return_value=False)
        load_mock.errors = False
        load_table_mock = mocker.MagicMock(
            name="load_table_from_json", return_value=load_mock
        )
        mocker.patch.object(metadata, "client")
        metadata.client.load_table_from_json = load_table_mock

        metadata.upload_table()

        metadata.client.load_table_from_json.assert_called_once()

    def test_upload_table_errors(self, mocker):
        """"""
        lake = None
        client = None
        testmode = None
        metadata = db.MeasurementMetadata(
            lake, "ci_measurements", 3, 10, client, testmode
        )
        metadata.json_data = {"nope": "nope"}
        metadata.job_config = None

        # With this we inject a mock chain
        # load_table_from_json is called, it returns a load_job
        # load_job.running() returns False
        load_mock = mocker.MagicMock(name="load_job")
        load_mock.running = mocker.MagicMock(name="running", return_value=False)

        load_mock.errors = True
        load_mock.error_results = "Error results"

        load_table_mock = mocker.MagicMock(
            name="load_table_from_json", return_value=load_mock
        )
        mocker.patch.object(metadata, "client")
        metadata.client.load_table_from_json = load_table_mock

        with pytest.raises(SystemError):
            metadata.upload_table()

    def test_upload_table_delayed(self, mocker):
        """"""
        lake = None
        client = None
        testmode = None
        metadata = db.MeasurementMetadata(
            lake, "ci_measurements", 3, 10, client, testmode
        )

        metadata.json_data = {"nope": "nope"}
        metadata.job_config = None

        mocker.patch("time.sleep")

        # With this we inject a mock chain
        # load_table_from_json is called, it returns a load_job
        # load_job.running() returns False
        load_mock = mocker.MagicMock(name="load_job")
        load_mock.running = mocker.MagicMock(
            name="running", side_effect=[True, True, True, False]
        )

        load_mock.errors = False

        load_table_mock = mocker.MagicMock(
            name="load_table_from_json", return_value=load_mock
        )
        mocker.patch.object(metadata, "client")
        metadata.client.load_table_from_json = load_table_mock

        metadata.upload_table()
        assert load_mock.running.call_count == 4

    def test_upload_metadata_b(self, mocker):
        """"""
        lake = os.path.expanduser("~/DataLakeTest")

        folders = [
            "results_1_unpack",
            "results_2_unpack",
            "results_4_unpack",
        ]
        client = "google"
        testmode = True

        metadata = db.MeasurementMetadata(
            lake, "ci_measurements", 3, 10, client, testmode
        )

        mocker.patch.object(metadata, "upload_table")

        metadata.postprocess(folders)

        metadata.update_table()

        metadata.upload_table.assert_called_once_with()

    def test_scrap_measurement_metadata(self):
        lake = os.path.expanduser("~/DataLakeTest")
        name = "system_test_1_metadata.json"
        file = os.path.join(lake, name)

        metadata = db.MeasurementMetadata(lake, "ci_measurements", 3, 10, None, None)

        ret, date, url, name, branch = metadata.scrap_data(file)

        assert ret == 1
        assert date == "2021-05-19T15:21:01Z"
        assert url == "https://github.com/abelikt/thin-edge.io/actions/runs/857323798"
        assert name == "system-test-workflow"
        assert branch == "continuous_integration"
