import download_all_artifacts as da


class TestDownloadArtifacts:
    def test_main(self, mocker):
        token = "token"
        user = "user"
        lake = "lake"
        mocker.patch.dict("os.environ", {"THEGHTOKEN": "token"})
        runs = [
            ["789311136", "93"],
            ["789260232", "92"],
            ["788264906", "91"],
        ]

        calls = [
            mocker.call("788264906", "91", token, lake, user),
            mocker.call("789260232", "92", token, lake, user),
            mocker.call("789311136", "93", token, lake, user),
        ]

        runmock = mocker.patch("download_all_artifacts.get_artifacts_for_runid")
        smock = mocker.patch(
            "download_all_artifacts.get_all_system_test_runs", return_value=runs
        )

        da.main(lake, user)

        assert runmock.call_count == len(runs)
        runmock.assert_has_calls(calls, any_order=True)
        smock.assert_called_with(token, lake, user)

    def test_get_all_runs_empty(self, mocker):

        url = "theurl"
        token = "token"
        user = "user"
        url = "https://api.github.com/repos/user/thin-edge.io/actions/runs"
        reqmock = mocker.MagicMock(name="reqmock")
        reqmock.text = '{"workflow_runs":{}}'
        rmock = mocker.patch("requests.get", return_value=reqmock)

        ret = da.get_all_runs(token, user)

        # Hint: this call finally runs the generator get_all runs
        thelist = list(ret)
        assert thelist == []
        rmock.assert_called_with(
            url, auth=mocker.ANY, params=mocker.ANY, headers=mocker.ANY
        )

    def test_get_all_runs(self, mocker):

        url = "theurl"
        token = "token"
        user = "user"
        url = "https://api.github.com/repos/user/thin-edge.io/actions/runs"

        reqmock = mocker.MagicMock(name="reqmock")
        reqmock.text = '{"workflow_runs":{ "one":"one", "two":"two"}}'
        reqmock2 = mocker.MagicMock(name="reqmock2")
        reqmock2.text = '{"workflow_runs":{}}'

        rmock = mocker.patch("requests.get", side_effect=[reqmock, reqmock2])
        ret = da.get_all_runs(token, user)

        # Hint: this call finally runs the generator get_all runs
        thelist = list(ret)

        assert thelist == [{"one": "one", "two": "two"}]
        rmock.assert_called_with(
            url, auth=mocker.ANY, params=mocker.ANY, headers=mocker.ANY
        )

    def test_get_all_system_test_runs_empty(self, mocker):

        mocker.patch(
            "download_all_artifacts.get_all_runs", return_value=[[{"name": "myname"}]]
        )
        ret = da.get_all_system_test_runs("token", "lake", "user")

        assert ret == []

    def test_get_all_system_test_runs_one(self, mocker):
        inject = [
            [
                {
                    "name": "system-test-workflow",
                    "run_number": "123",
                    "id": "456",
                    "workflow_id": "678",
                }
            ]
        ]

        getmock = mocker.patch(
            "download_all_artifacts.get_all_runs", return_value=inject
        )
        lake = "lake"
        user = "user"
        mock = mocker.mock_open(read_data="data")

        mocker.patch("download_all_artifacts.open", mock)

        ret = da.get_all_system_test_runs("token", lake, user)

        assert ret == [
            (
                "456",
                "123",
            )
        ]
        getmock.assert_called_with("token", "user")

    def test_get_artifacts_for_runid_no_artifacts(self, mocker):
        inject = {"artifacts": []}
        dmock = mocker.patch("download_all_artifacts.download_artifact")
        mocker.patch("__main__.open")
        mocker.patch("requests.get")
        # instead of fiddling around with the return value of requests.get
        # we just patch json.loads
        mocker.patch("json.loads", return_value=inject)
        runid = 42
        run_number = 43
        token = "token"
        lake = "lake"
        user = "user"
        mocker.patch("download_all_artifacts.open")

        da.get_artifacts_for_runid(runid, run_number, token, lake, user)
        dmock.assert_not_called()

    def test_get_artifacts_for_runid_one_artifact(self, mocker):
        inject = {"artifacts": [{"name": "bob", "archive_download_url": "theurl"}]}
        dmock = mocker.patch("download_all_artifacts.download_artifact")
        mocker.patch("__main__.open")
        reqmock = mocker.patch("requests.get")
        # instead of fiddling around with the return value of requests.get
        # we just patch json.loads
        mocker.patch("json.loads", return_value=inject)
        runid = 42
        run_number = 43
        token = "token"
        lake = "lake"
        user = "user"
        mocker.patch("download_all_artifacts.open")
        url = f"https://api.github.com/repos/{user}/thin-edge.io/actions/runs/{runid}/artifacts"

        da.get_artifacts_for_runid(runid, run_number, token, lake, user)

        dmock.assert_called_once_with("theurl", "bob", 43, token, lake, user)
        reqmock.assert_called_with(url, auth=mocker.ANY, headers=mocker.ANY)

    def test_download_artifact(self, mocker):

        url = "theurl"
        run_number = 2
        name = "results_"
        token = "token"
        lake = "lake"
        user = "user"
        rmock = mocker.patch("requests.get")
        mocker.patch("os.path.exists", return_value=False)
        mocker.patch("download_all_artifacts.open")

        da.download_artifact(url, name, run_number, token, lake, user)

        rmock.assert_called_once()

        rmock.assert_called_with(
            url,
            auth=mocker.ANY,  # can't really check Http Auth from here
            headers=mocker.ANY,
            stream=True,
        )
