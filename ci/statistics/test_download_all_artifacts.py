

import download_all_artifacts as da

class TestDownloadArtifacts:

    def test_main(self, mocker):
        runs = [
            ["789311136", "93"],
            ["789260232", "92"],
            ["788264906", "91"],
        ]

        runmock= mocker.patch("download_all_artifacts.get_artifacts_for_runid")
        smock = mocker.patch("download_all_artifacts.get_all_system_test_runs",
            return_value = runs)

        da.main()
        assert runmock.call_count == len(runs)




    def test_get_all_system_test_runs(self, mocker):

        mocker.patch("download_all_artifacts.get_all_runs")
        da.get_all_system_test_runs("token")