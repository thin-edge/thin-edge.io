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

# system-test-workflow_A.yml
# ci_system-test-workflow_A.zip
# system-test-workflow-A_261.json
# ci_system-test-report_A.xml


runners_cfg = {
    "michael":{     "repo":"abelikt", "prefix":"ci_system-test-workflow",   "report":"ci_system-test-report",  "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
    "offsitea":{    "repo":"abelikt", "prefix":"ci_system-test-workflow_A", "report":"ci_system-test-report_A",  "tests":["all", "apt", "apama", "docker", "sm", ] },
    "offsiteb":{    "repo":"abelikt", "prefix":"ci_system-test-workflow_B", "report":"ci_system-test-report_B",  "tests":["all", "apt", "apama", "docker", "sm", ] },
    "offsitec":{    "repo":"abelikt", "prefix":"ci_system-test-workflow_C", "report":"ci_system-test-report_C",  "tests":["all", "apt", "apama", "docker", "sm", ] },
    "offsited":{    "repo":"abelikt", "prefix":"ci_system-test-workflow_D", "report":"ci_system-test-report_D",   "tests":["all", "apt", "apama", "docker", "sm", ] },

    "sag":{         "repo":"thin-edge", "prefix":"sag_system-test-workflow",  "report":"sag_system-test-report_workflow",  "tests":["all" ] },
    "offsite-sag":{ "repo":"thin-edge", "prefix":"sag_system-test-offsite",   "report":"sag_system-test-report_offsite",   "tests":["all", "apt", "docker", "sm", ] },
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

def download(workflow, repo, folders, simulate=False):
    # Download and unzip results from test workflows

    # ./download_workflow_artifact.py abelikt system-test-workflow_A.yml -o ci_system-test-workflow_A;
    # ./download_workflow_artifact.py abelikt system-test-workflow_B.yml -o ci_system-test-workflow_B;
    # unzip -q -o -d ci_system-test-workflow_B ci_system-test-workflow_B.zip

    w=workflow

    y = w.replace(".yml",".zip")
    name = w.replace(".yml","")
    print(w, y)
    cmd=f"./download_workflow_artifact.py {repo} {w} -o {name};"
    print(cmd)

    if not simulate:
        sub=subprocess.run(cmd, shell=True)
        sub.check_returncode()

    assert os.path.exists(y)

    cmd =f"unzip -q -o -d {name} {y}"
    print(cmd)

    sub=subprocess.run(cmd, shell=True)
    sub.check_returncode()


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

    simulate = not download_reports

    for key in runners.keys():
        print( "Key", key, "Repo", runners[key]["repo"] )
        download( runners[key]["prefix"]+".yml", runners[key]["repo"], runners[key]["report"], simulate=simulate)

    print(runners.keys())

    for key in runners.keys():
        postprocess_runner( runners[key] )

    postprocess()

if __name__=="__main__":
    main(runners_cfg, download_reports=True)

