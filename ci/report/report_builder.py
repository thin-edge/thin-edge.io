#!/bin/bash

# This solution is far from perfect
# TODO Export configuration to separate config file
# TODO Add Command line interface
# TODO Add an additional report to store the sources (run-id, date, runner)
# TODO return non zero exit code when there was an issue

import os
import sys
import subprocess

# set -e
#
# python3 -m venv ~/env-pysys
# source ~/env-pysys/bin/activate
# pip3 install -r tests/requirements.txt
#

# system-test-workflow_A.yml
# ci_system-test-workflow_A.zip
# system-test-workflow-A_261.json
# ci_system-test-report_A.xml


runners_cfg = {
    "michael": {
        "repo": "abelikt",
        "workflow": "system-test-workflow.yml",
        "tests": ["all", "apt", "apama", "docker", "sm", "analytics"],
    },
    "offsitea": {
        "repo": "abelikt",
        "workflow": "system-test-workflow_A.yml",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    "offsiteb": {
        "repo": "abelikt",
        "workflow": "system-test-workflow_B.yml",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    "offsitec": {
        "repo": "abelikt",
        "workflow": "system-test-workflow_C.yml",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    "offsited": {
        "repo": "abelikt",
        "workflow": "system-test-workflow_D.yml",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    "sag": {
        "repo": "thin-edge",
        "workflow": "system-test-workflow.yml",
        "tests": ["all"],
    },
    "offsite-sag": {
        "repo": "thin-edge",
        "workflow": "system-test-offsite.yml",
        "tests": [
            "all",
            "apt",
            "docker",
            "sm",
        ],
    },
}


def cleanup(download_reports):

    folders = [
        "*.xml",
        "*.html",
        "system-test-workflow",
        "system-test-workflow_A",
        "system-test-workflow_B",
        "system-test-workflow_C",
        "system-test-workflow_D",
        "ci_system-test-workflow",
        "ci_system-test-workflow_A",
        "ci_system-test-workflow_B",
        "ci_system-test-workflow_C",
        "ci_system-test-workflow_D",
        "sag_system-test-workflow",
        "sag_system-test-offsite",
    ]

    if download_reports:
        folders.append("*.zip")
        folders.append("*.json")

    for folder in folders:
        cmd = "rm -rf " + folder
        print(cmd)
        sub = subprocess.run(cmd, shell=True)

        # sub.check_returncode()
        if sub.returncode != 0:
            print("Warning command failed:", cmd)


def download(workflow, repo, simulate=False):
    # Download and unzip results from test workflows

    # ./download_workflow_artifact.py abelikt system-test-workflow_A.yml -o ci_system-test-workflow_A;
    # ./download_workflow_artifact.py abelikt system-test-workflow_B.yml -o ci_system-test-workflow_B;
    # unzip -q -o -d ci_system-test-workflow_B ci_system-test-workflow_B.zip

    name = repo + "_" + workflow.replace(".yml", "")
    filename = name + ".zip"

    print(name)
    cmd = f"../download_workflow_artifact.py {repo} {workflow} -o {name}"
    print(cmd)

    if not simulate:
        sub = subprocess.run(cmd, shell=True)
        sub.check_returncode()

    assert os.path.exists(filename)

    cmd = f"unzip -q -o -d {name} {filename}"
    print(cmd)

    sub = subprocess.run(cmd, shell=True)
    sub.check_returncode()


def postprocess_runner(runner):
    # Postprocess results

    # Postporcess results for the onsite runner onsite at Michael
    workflow = runner["workflow"]
    repo = runner["repo"]
    tests = runner["tests"]

    print(f"Processing: {workflow} ")

    tags = ["all", "apt", "apama", "docker", "sm", "analytics"]
    files = ""

    name = repo + "_" + workflow.replace(".yml", "")

    for tag in tags:
        if tag in tests:
            folder = f"{name}/PySys/pysys_junit_xml_{tag}"
            if os.path.exists(folder):
                files += f"{name}/PySys/pysys_junit_xml_{tag}/*.xml "
            else:
                raise SystemError("Folder Expected", folder)

    cmd = f"junitparser merge {files} { name }.xml"

    print(cmd)

    sub = subprocess.run(cmd, shell=True)
    sub.check_returncode()

    print(f"junit2html {name}.xml")


def postprocess(runners):

    # Create a combined report matrix from all report sources

    files = ""

    for key in runners.keys():
        workflow = runners[key]["workflow"]
        repo = runners[key]["repo"]
        name = repo + "_" + workflow.replace(".yml", ".xml")
        files += " " + name

    print("Files:  ", files)


    # Print summary matrix

    cmd = f"junit2html --summary-matrix {files}"
    print(cmd)

    os.system(cmd)

    cmd = f"junit2html --summary-matrix {files} > report.out"
    print(cmd)
    os.system(cmd)

    # # Build report matrix
    cmd = f"junit2html --report-matrix report-matrix.html {files}"
    print(cmd)

    os.system(cmd)

    # Zip everything
    # zip report.zip *.html *.json


def main(runners, download_reports=True):

    cleanup(download_reports)

    simulate = not download_reports

    for key in runners.keys():
        print("Key", key, "Repo", runners[key]["repo"])
        download(runners[key]["workflow"], runners[key]["repo"], simulate=simulate)

    print(runners.keys())

    for key in runners.keys():
        postprocess_runner(runners[key])

    postprocess(runners)


if __name__ == "__main__":

    download_reports=True
    if download_reports:
        os.rmdir("report")
        os.mkdir("report")
    os.chdir("report")

    main(runners_cfg, download_reports=download_reports)
