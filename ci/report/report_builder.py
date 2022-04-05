#!/bin/bash

# This solution is far from perfect
# TODO Make it more flexible
# TODO Make it more obvious what is going on here TODO Host this report somewhere
# TODO Add an additional report to store the sources (run-id, date, runner)
# TODO Decide on what to do if we have failures or skipped workflows

import os
import sys
import subprocess

# set -e
#
# python3 -m venv ~/env-pysys
# source ~/env-pysys/bin/activate
# pip3 install -r tests/requirements.txt
#

workflows_abel = ["system-test-workflow_A.yml",
"system-test-workflow_B.yml",
"system-test-workflow_C.yml",
"system-test-workflow_D.yml",
"system-test-workflow.yml"]


workflows_sag = ["system-test-workflow.yml", "system-test-offsite.yml"]

folders_abel=["ci_system-test-workflow",
            "ci_system-test-workflow_A",
            "ci_system-test-workflow_B",
            "ci_system-test-workflow_C",
            "ci_system-test-workflow_D"]

folders_sag= ["sag_system-test-workflow", "sag_system-test-offsite"]


runners_cfg = {
    "michael":{     "prefix":"ci_system-test-workflow",   "report":"ci_system-test-report",  "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
    "offsitea":{    "prefix":"ci_system-test-workflow_A", "report":"ci_system-test-report_A",  "tests":["all", "apt", "apama", "docker", "sm", ] },
    "offsiteb":{    "prefix":"ci_system-test-workflow_B", "report":"ci_system-test-report_B",  "tests":["all", "apt", "apama", "docker", "sm", ] },
    "offsitec":{    "prefix":"ci_system-test-workflow_C", "report":"ci_system-test-report_C",  "tests":["all", "apt", "apama", "docker", "sm", ] },
    "offsited":{    "prefix":"ci_system-test-workflow_D", "report":"ci_system-test-report_D",   "tests":["all", "apt", "apama", "docker", "sm", ] },

    "sag":{         "prefix":"sag_system-test-workflow",  "report":"sag_system-test-report_workflow",  "tests":["all" ] },
    "offsite-sag":{ "prefix":"sag_system-test-offsite",   "report":"sag_system-test-report_offsite",   "tests":["all", "apt", "docker", "sm", ] },
        }

def cleanup(download_reports):

    folders =     [
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
    "sag_system-test-offsite"]

    if download_reports:
        folders.append("*.zip")
        folders.append("*.json")

    for folder in folders:
        cmd = "rm -rf "+ folder
        print(cmd)
        sub = subprocess.run(cmd, shell=True)

        #sub.check_returncode()
        if sub.returncode != 0:
            print("Warning command failed:", cmd)

def download(workflows, repo, folders, simulate=False):
    # Download and unzip results from test workflows

    if repo == "abelikt":
        prefix= "ci_"
    elif repo =="thin-edge":
        prefix="sag_"
    else:
        raise SystemError

    for w in workflows:
        y = w.replace(".yml",".zip")
        name = w.replace(".yml","")
        print(w, y)
        cmd=f"./download_workflow_artifact.py {repo} {w} -o {prefix}{name};"
        print(cmd)

        if not simulate:
            sub=subprocess.run(cmd, shell=True)

        cmd =f"unzip -q -o -d {prefix}{name} {prefix}{y}"
        print(cmd)

        sub=subprocess.run(cmd, shell=True)

    # Doublecheck if our result folders are there

    for f in folders:
        print("Checking folder", f)
        assert os.path.exists(f)


def postprocess_runner(runner):
    # Postprocess results


    # Postporcess results for the onsite runner onsite at Michael
        prefix = runner["prefix"]
        report = runner ["report"]
        tests = runner["tests"]

        print(f"Processing: {prefix} ")

        tags = ["all", "apt", "apama", "docker", "sm", "analytics"]
        files = ""

        for tag in tags:
            if tag in tests:
                folder = f"{prefix}/PySys/pysys_junit_xml_{tag}"
                if os.path.exists(folder):
                    files += f"{prefix}/PySys/pysys_junit_xml_{tag}/*.xml "
                else:
                    raise SystemError("Folder Expected", folder)

        cmd = f"junitparser merge {files} { report }.xml"

        print(cmd)

        sub=subprocess.run(cmd, shell=True)
        sub.check_returncode()

        print(f"junit2html {report}.xml")

def postprocess():

    # Create a combined report matrix from all report sources
    OUT = "ci_system-test-report"
    SAGOUT = "sag_system-test-report"

    XMLFILES = (
        OUT
        + ".xml "
        + OUT
        + "_A.xml "
        + OUT
        + "_B.xml "
        + OUT
        + "_C.xml "
        + OUT
        + "_D.xml "
        + SAGOUT
        + "_offsite.xml "
        + SAGOUT
        + "_workflow.xml"
    )

    print("Files:  ", XMLFILES)

    expect = "ci_system-test-report.xml ci_system-test-report_A.xml ci_system-test-report_B.xml ci_system-test-report_C.xml ci_system-test-report_D.xml sag_system-test-report_offsite.xml sag_system-test-report_workflow.xml"

    print("Expected: ", expect)

    assert XMLFILES == expect

    # Print summary matrix

    cmd = f"junit2html --summary-matrix {XMLFILES}"
    print(cmd)

    expect = "junit2html --summary-matrix ci_system-test-report.xml ci_system-test-report_A.xml ci_system-test-report_B.xml ci_system-test-report_C.xml ci_system-test-report_D.xml sag_system-test-report_offsite.xml sag_system-test-report_workflow.xml"

    assert expect == cmd

    os.system(cmd)

    cmd = f"junit2html --summary-matrix {XMLFILES} > report.out"
    print(cmd)
    os.system(cmd)

    # # Build report matrix
    cmd = f"junit2html --report-matrix report-matrix.html {XMLFILES}"
    print(cmd)

    expect = "junit2html --report-matrix report-matrix.html ci_system-test-report.xml ci_system-test-report_A.xml ci_system-test-report_B.xml ci_system-test-report_C.xml ci_system-test-report_D.xml sag_system-test-report_offsite.xml sag_system-test-report_workflow.xml"

    assert expect == cmd

    os.system(cmd)


    # Zip everything
    # zip report.zip *.html *.json

def main(runners, download_reports=True):

    cleanup(download_reports)

    simulate = True

    download( workflows_abel, "abelikt", folders_abel, simulate=simulate)
    download( workflows_sag, "thin-edge", folders_sag, simulate=simulate)


    print(runners.keys())

    for key in runners.keys():
        postprocess_runner( runners[key] )

    postprocess()

if __name__=="__main__":
    main(runners_cfg, download_reports=False)

