#!/usr/bin/python3

# This solution is far from perfect
# TODO Export configuration to separate config file
# TODO Add an additional report to store the sources (run-id, date, runner)
# TODO return non zero exit code when there was an issue

import argparse
import os
import sys
import subprocess
import shutil

# Exemplary call
#
# python3 -m venv ~/env-pysys
# source ~/env-pysys/bin/activate
# pip3 install -r tests/requirements.txt
#
# ./report_builder.py abelikt commit-workflow-allinone.yml
# ./report_builder.py abelikt commit-workflow-allinone.yml --download

runners_cfg = [
    {
        "name": "offsite_mythica",
        "repo": "abelikt",
        "archive": "results_pysys_offsite_mythica.zip",
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
        "archive": "results_pysys_offsite_mythicb.zip",
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
        "archive": "results_pysys_offsite_mythicc.zip",
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
        "archive": "results_pysys_offsite_mythicd.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
]


def download_results(repo, workflow):
    # Download and unzip results from test workflows

    cmd = f"../download_workflow_artifact.py {repo} {workflow} --filter results"
    print(cmd)
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


def postprocess(runners):

    # Create a combined report matrix from all report sources

    files = ""

    for runner in runners:
        name = runner["name"] + ".xml"
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
    cmd = "zip report.zip *.html *.json"
    print(cmd)
    sub = subprocess.run(cmd, shell=True)
    sub.check_returncode()

def main(runners, repo, workflow, download_reports=True):

    # TODO make this more flexible
    # path = os.path.dirname(os.path.realpath(__file__))
    # os.chdir(path)

    #if download_reports:
    #    shutil.rmtree("report")
    #    os.mkdir("report")
    #os.chdir("report")

    if download_reports:
        download_results("abelikt", "commit-workflow-allinone.yml")

        for runner in runners:
            unpack_reports(runner)

    for runner in runners:
        postprocess_runner(runner)

    postprocess(runners)


if __name__ == "__main__":

    parser = argparse.ArgumentParser()
    parser.add_argument("repo", type=str, help="GitHub repository")
    parser.add_argument("workflow", type=str, help="Name of workflow")
    parser.add_argument('--download', action='store_true')

    args = parser.parse_args()

    repo = args.repo
    workflow = args.workflow
    download = args.download

    main(runners_cfg, repo, workflow, download_reports=download)

