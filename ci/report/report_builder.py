#!/usr/bin/python3

"""
Build a complete report for all our runners

Exemplary call

python3 -m venv ~/env-builder
source ~/env-builder/bin/activate
pip3 install junitparser
pip3 install junit2html python3 -m venv ~/env-pysys

./report_builder.py thin-edge ci_pipeline.yml
./report_builder.py thin-edge ci_pipeline.yml --download

TODO Export configuration to separate config file

"""

import argparse
import os
import subprocess
import shutil


runners_cfg = [
    {
        "name": "results_pysys_offsite_m32sd10a",
        "repo": "thin-edge",
        "archive": "results_pysys_offsite_m32sd10a.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    {
        "name": "results_pysys_offsite_m32sd10b",
        "repo": "thin-edge",
        "archive": "results_pysys_offsite_m32sd10b.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    {
        "name": "results_pysys_offsite_m32sd10c",
        "repo": "thin-edge",
        "archive": "results_pysys_offsite_m32sd10c.zip",
        "tests": [
            "all",
            "apt",
            "apama",
            "docker",
            "sm",
        ],
    },
    {
        "name": "results_pysys_offsite_m32sd10d",
        "repo": "thin-edge",
        "archive": "results_pysys_offsite_m32sd10d.zip",
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
    """Download and unzip results from test workflows"""

    scriptfolder = os.path.dirname(os.path.realpath(__file__))

    cmd = (
        f"{scriptfolder}/download_workflow_artifact.py {repo} {workflow}"
        + " -o ./ --filter results --ignore"
    )
    print(cmd)
    subprocess.run(cmd, shell=True, check=True)


def unpack_reports(runner):
    """Unpack reports mentioned in the runner configuration"""

    assert os.path.exists(runner["archive"])
    name = runner["name"]
    archive = runner["archive"]
    cmd = f"unzip -q -o -d {name} {archive}"
    print(cmd)
    subprocess.run(cmd, shell=True, check=True)


def postprocess_runner(runner):
    """Postprocess results from a runner.
    Fails if a test folder is missing that is mentioned in the runner
    configuration.
    """

    name = runner["name"]
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
    subprocess.run(cmd, shell=True, check=True)


def postprocess(runners):
    """Create a combined reports from all sources"""

    files = ""

    for runner in runners:
        name = runner["name"] + ".xml"
        files += " " + name

    print("Files:  ", files)

    # Print summary matrix
    cmd = f"junit2html --summary-matrix {files}"
    print(cmd)
    subprocess.run(cmd, shell=True, check=True)

    # Merge all reports
    cmd = f"junitparser merge {files} all_reports.xml"
    print(cmd)
    subprocess.run(cmd, shell=True, check=True)

    # Build report matrix
    cmd = f"junit2html --report-matrix report-matrix.html {files}"
    print(cmd)
    subprocess.run(cmd, shell=True, check=True)

    # Zip everything
    cmd = "zip report.zip *.html *.json"
    print(cmd)
    subprocess.run(cmd, shell=True, check=True)


def main(runners, repo, workflow, folder, download_reports=True):
    """Main entry point to build the reports"""

    if download_reports:
        # delete folder contents
        shutil.rmtree(folder, ignore_errors=True)
        os.mkdir(folder)
    else:
        # reuse folder with downloaded zip files
        pass

    os.chdir(folder)

    if download_reports:
        download_results(repo, workflow)

        for runner in runners:
            unpack_reports(runner)

    for runner in runners:
        postprocess_runner(runner)

    postprocess(runners)


if __name__ == "__main__":

    parser = argparse.ArgumentParser()
    parser.add_argument("repo", type=str, help="GitHub repository")
    parser.add_argument("workflow", type=str, help="Name of workflow")
    parser.add_argument(
        "--folder",
        type=str,
        help="Working folder (Default ./report )",
        default="./report",
    )
    parser.add_argument("--download", action="store_true", help="Download reports")

    args = parser.parse_args()

    main(
        runners_cfg,
        args.repo,
        args.workflow,
        folder=args.folder,
        download_reports=args.download,
    )
