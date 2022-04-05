#!/bin/bash

# This solution is far from perfect
# TODO Export configuration to separate config file
# TODO Add Command line interface
# TODO Add an additional report to store the sources (run-id, date, runner)
# TODO return non zero exit code when there was an issue

import os
import sys
import subprocess
import shutil

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

# commit-workflow-allinone_results_a_35.zip
# commit-workflow-allinone_results_b_35.zip
# commit-workflow-allinone_results_c_35.zip
# commit-workflow-allinone_results_d_35.zip


runners_cfg = [
    {
        "name": "offsite_mythica",
        "repo": "abelikt",
        "archive": "commit-workflow-allinone_results_a_35.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    {
        "name": "offsite_mythicb",
        "repo": "abelikt",
        "archive": "commit-workflow-allinone_results_b_35.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    {
        "name": "offsite_mythicc",
        "repo": "abelikt",
        "archive": "commit-workflow-allinone_results_c_35.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    {
        "name": "offsite_mythicd",
        "repo": "abelikt",
        "archive": "commit-workflow-allinone_results_d_35.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
]


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


def download(repo, workflow, simulate=False):
    # Download and unzip results from test workflows

    # ./download_workflow_artifact.py abelikt system-test-workflow_A.yml -o ci_system-test-workflow_A;
    # ./download_workflow_artifact.py abelikt system-test-workflow_B.yml -o ci_system-test-workflow_B;
    # unzip -q -o -d ci_system-test-workflow_B ci_system-test-workflow_B.zip

    # ./download_workflow_artifact.py abelikt commit-workflow-allinone.yml --filter result

    #name = repo + "_" + workflow.replace(".yml", "")
    #filename = name + ".zip"

    #print(name)
    #cmd = f"../download_workflow_artifact.py {repo} {workflow} -o {name}"
    cmd = f"../download_workflow_artifact.py {repo} {workflow} --filter result"
    print(cmd)

    if not simulate:
        sub = subprocess.run(cmd, shell=True)
        sub.check_returncode()


def unpack_reports(runner):
    assert os.path.exists(runner["archive"])
    name = runner["name"]
    archive = runner["archive"]
    cmd = f"unzip -q -o -d {name} {archive}"
    print(cmd)

    sub = subprocess.run(cmd, shell=True)
    sub.check_returncode()


def postprocess_runner(runner):
    # Postprocess results

    # Postporcess results for the onsite runner onsite at Michael
    name = runner["name"]
    repo = runner["repo"]
    tests = runner["tests"]

    print(f"Processing: {name} ")

    tags = ["all", "apt", "apama", "docker", "sm", "analytics"]
    files = ""

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

    for runner in runners:
        name = runner["name"] + ".xml"
        repo = runner["repo"]
        files += " " + name

    print("Files:  ", files)

    # Print summary matrix
    cmd = f"junit2html --summary-matrix {files}"
    print(cmd)
    sub = subprocess.run(cmd, shell=True)
    sub.check_returncode()

    # Build report matrix
    cmd = f"junit2html --report-matrix report-matrix.html {files}"
    print(cmd)
    sub = subprocess.run(cmd, shell=True)
    sub.check_returncode()

    # Zip everything
    cmd="zip report.zip *.html *.json"
    print(cmd)
    sub = subprocess.run(cmd, shell=True)
    sub.check_returncode()


def main(runners, download_reports=True):

    #cleanup(download_reports)

    simulate = not download_reports

    # for key in runners.keys():
    #    print("Key", key, "Repo", runners[key]["repo"])
    #    download(runners[key]["workflow"], runners[key]["repo"], simulate=simulate)

    download("abelikt", "commit-workflow-allinone.yml" ,simulate);

    for runner in runners:
        print("Runner", runner, "Repo", runner["repo"])
        unpack_reports(runner)

    for runner in runners:
        postprocess_runner(runner)

    postprocess(runners)


if __name__ == "__main__":

    download_reports = False

    if download_reports:
        shutil.rmtree("report")
        os.mkdir("report")
    os.chdir("report")

    main(runners_cfg, download_reports=download_reports)
