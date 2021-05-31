#!/usr/bin/python3
"""Download all artifacts from GitHub

See also here
https://docs.github.com/en/rest/reference/actions#download-an-artifact
"""

# Hint: Without the auth token we get this error message:
#
# {'message': "API rate limit exceeded. (But here's the good news:
# Authenticated requests get a higher rate limit. Check out the
# documentation for more details.)", 'documentation_url':
# 'https://docs.github.com/rest/overview/resources-in-the-rest-api#rate-limiting'}

# TODO: Add some heuristic to know if we have most of the data
# available and can skip downloading

import json
import requests
import os
import sys
from requests.auth import HTTPBasicAuth

lake = os.path.expanduser("~/DataLake")


def download_artifact(url, name, run_number, token):
    headers = {"Accept": "application/vnd.github.v3+json"}

    auth = HTTPBasicAuth("abelikt", token)

    print(f"Will try {lake}/{name}.zip aka results_{run_number}'")

    # Repair old names
    if name == "results_":
        name = f"results_{run_number}"
    elif name == "results_$RUN_NUMBER":
        name = f"results_{run_number}"
    elif name == "results":
        name = f"results_{run_number}"
    elif name == "results_$GITHUB_RUN_ID":
        name = f"results_{run_number}"
    else:
        assert name == f"results_{run_number}"

    artifact_filename = f"{lake}/{name}.zip"

    if os.path.exists(artifact_filename):
        print(f"Skipped {lake}/{name}.zip")
        return

    req = requests.get(url, auth=auth, headers=headers, stream=True)

    with open(os.path.expanduser(artifact_filename), "wb") as fd:
        for chunk in req.iter_content(chunk_size=128):
            fd.write(chunk)
        print(f"Downloaded {lake}/{name}.zip")


def get_artifacts_for_runid(runid, run_number, token):
    """Download artifacts for a given runid"""
    # Here we need the runid and we get the artifact id

    # Manual
    # https://github.com/abelikt/thin-edge.io/actions/runs/828065682
    # curl -H "Accept: application/vnd.github.v3+json" -u abelikt:$TOKEN
    # -L https://api.github.com/repos/abelikt/thin-edge.io/actions/runs/828065682/artifacts

    url = f"https://api.github.com/repos/abelikt/thin-edge.io/actions/runs/{runid}/artifacts"
    headers = {"Accept": "application/vnd.github.v3+json"}

    auth = HTTPBasicAuth("abelikt", token)
    # per_page

    req = requests.get(url, auth=auth, headers=headers)
    stuff = json.loads(req.text)
    # print(json.dumps(stuff, indent=4))

    with open(
        os.path.expanduser(f"{lake}/results_{run_number}_metadata.json"), "w"
    ) as ofile:
        ofile.write(json.dumps(stuff, indent=4))

    artifacts = stuff["artifacts"]

    if len(artifacts) > 0:
        artifact_name = artifacts[0]["name"]
        artifact_url = artifacts[0]["archive_download_url"]
        print(artifact_url)
        download_artifact(artifact_url, artifact_name, run_number, token)
        return artifact_url
    else:
        print("No Artifact attached")


def get_all_runs(token):
    """Download all GitHub Actions workflow runs.
    Generator function that returns the next 50 runs from the web-ui
    as list of dictionaries.
    """

    # manual
    # curl -H "Accept: application/vnd.github.v3+json" -u abelikt:$TOKEN
    # -L https://api.github.com/repos/abelikt/thin-edge.io/actions/runs

    url = f"https://api.github.com/repos/abelikt/thin-edge.io/actions/runs"
    headers = {"Accept": "application/vnd.github.v3+json"}

    auth = HTTPBasicAuth("abelikt", token)

    index = 0  # 0 and 1 seem to have an identical meaning here
    gathered = 0
    empty = False

    while not empty:
        print(f"Request {index}")
        params = {"per_page": "50", "page": index}
        req = requests.get(url, params=params, auth=auth, headers=headers)
        stuff = json.loads(req.text)
        # print(req.text)
        # print(json.dumps(stuff, indent=4))

        # for s in stuff:
        #    print(s)
        # print(json.dumps(stuff['workflow_runs'][0], indent=4))

        # print("Total Count", stuff['total_count'])
        try:
            read = len(stuff["workflow_runs"])
        except KeyError as ke:
            print("Error", ke, stuff)
            print("Message from GitHub: ", stuff["message"])
            sys.exit(1)

        if read == 0:
            print("Empty")
            return {}
        else:
            print(f"Read {read} entries")

        # for s in stuff['workflow_runs']:
        #    if s['name'] == 'system-test-workflow':
        #        print(s['id'], s['run_number'])
        index += 1
        yield stuff["workflow_runs"]


def get_all_system_test_runs(token):
    """Returns als system test runs as list of run_id and number"""
    system_test_runs = []
    for i in get_all_runs(token):
        for test_run in i:
            if test_run["name"] == "system-test-workflow":
                # print( j['id'], j['run_number'])
                # print(json.dumps(j, indent=4))
                run_number = test_run["run_number"]
                with open(
                    os.path.expanduser(
                        f"{lake}/system_test_{run_number}_metadata.json"
                    ),
                    "w",
                ) as ofile:
                    ofile.write(json.dumps(test_run, indent=4))
                print(
                    f"Found System Test Run with id {test_run['id']} run number {run_number} workflow id {test_run['workflow_id']}"
                )
                system_test_runs.append((test_run["id"], run_number))

    print(f"Found {len(system_test_runs)} test_runs ")

    return system_test_runs


def main():
    """main entry point"""
    token = None

    if "THEGHTOKEN" in os.environ:
        token = os.environ["THEGHTOKEN"]
    else:
        print("Error environment variable THEGHTOKEN not set")
        sys.exit(1)

    system_test_runs = get_all_system_test_runs(token)

    for s in system_test_runs:
        artifact = get_artifacts_for_runid(s[0], s[1], token)
        #print(artifact)


if __name__ == "__main__":
    main()
