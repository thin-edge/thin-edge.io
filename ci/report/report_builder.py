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
# ./report_builder.py abelikt ci_pipeline.yml
# ./report_builder.py abelikt ci_pipeline.yml --download

runners_cfg = [
    {
        "name": "results_pysys_offsite_mythica",
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
        "name": "results_pysys_offsite_mythicb",
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
        "name": "results_pysys_offsite_mythicc",
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
        "name": "results_pysys_offsite_mythicd",
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


def download_results(repo, workflow, folder):
    # Download and unzip results from test workflows

    scriptfolder=os.path.dirname(os.path.realpath(__file__))

    cmd = (
        f"{scriptfolder}/download_workflow_artifact.py {repo} {workflow} -o ./ --filter results --ignore"
    )
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


def main(runners, repo, workflow, folder, download_reports=True):

#    if folder:
#        os.chdir(folder)
#    else:
#        # Switch to script path
#        path = os.path.dirname(os.path.realpath(__file__))
#        os.chdir(path)

    if download_reports:
        # delete folder contents
        shutil.rmtree(folder, ignore_errors=True)
        os.mkdir(folder)
    else:
        # reuse folder with downloaded zip files
        pass

    os.chdir(folder)

    if download_reports:
        download_results("abelikt", "ci_pipeline.yml", folder)

        for runner in runners:
            unpack_reports(runner)

    for runner in runners:
        postprocess_runner(runner)

    postprocess(runners)


if __name__ == "__main__":

    parser = argparse.ArgumentParser()
    parser.add_argument("repo", type=str, help="GitHub repository")
    parser.add_argument("workflow", type=str, help="Name of workflow")
    parser.add_argument("--folder", type=str, help="Working folder (Default ./report )", default="./report")
    parser.add_argument(
        "--download", action="store_true", help="Download reports"
    )

    args = parser.parse_args()

    repo = args.repo
    workflow = args.workflow
    download = args.download
    folder = args.folder

    print(args)

    main(runners_cfg, repo, workflow, folder=folder, download_reports=download)
