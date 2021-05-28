

import download_all_artifacts as da

class TestDownloadArtifacts:

    def test_main(self, mocker):
        token = "token"
        mocker.patch.dict('os.environ', {'THEGHTOKEN': 'token'})
        runs = [
            ["789311136", "93"],
            ["789260232", "92"],
            ["788264906", "91"],
        ]

        calls = [
            mocker.call("788264906", "91", token),
            mocker.call("789260232", "92", token),
            mocker.call("789311136", "93", token),
        ]

        runmock= mocker.patch("download_all_artifacts.get_artifacts_for_runid")
        smock = mocker.patch("download_all_artifacts.get_all_system_test_runs",
            return_value = runs)

        da.main()
        assert runmock.call_count == len(runs)
        runmock.assert_has_calls(calls, any_order=True)

    def test_get_all_system_test_runs_empty(self, mocker):

        mocker.patch("download_all_artifacts.get_all_runs",
            return_value=[[ {"name":"myname"} ]] )

        ret = da.get_all_system_test_runs("token")

        assert ret == []

    def test_get_all_system_test_runs_one(self, mocker):
        inject = [[ {
                "name":"system-test-workflow",
                "run_number": "123",
                "id":"456",
                "workflow_id":"678"
                } ]]

        mocker.patch("download_all_artifacts.get_all_runs",
            return_value=inject )

        ret = da.get_all_system_test_runs("token")

        assert ret == [("456","123",)]

