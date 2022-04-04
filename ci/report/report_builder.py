#!/bin/bash

# This solution is far from perfect
# TODO Make it more flexible
# TODO Make it more obvious what is going on here TODO Host this report somewhere
# TODO Add an additional report to store the sources (run-id, date, runner)
# TODO Decide on what to do if we have failures or skipped workflows

import os
import sys

# set -e
#
# python3 -m venv ~/env-pysys
# source ~/env-pysys/bin/activate
# pip3 install -r tests/requirements.txt
#
# cd ci/report/
#
# # Cleanup
#
# rm -f *.zip
# rm -f *.xml
# rm -f *.html
# rm -f *.json
# rm -rf system-test-workflow
# rm -rf system-test-workflow_A
# rm -rf system-test-workflow_B
# rm -rf system-test-workflow_C
# rm -rf system-test-workflow_D
#
# rm -rf ci_system-test-workflow
# rm -rf ci_system-test-workflow_A
# rm -rf ci_system-test-workflow_B
# rm -rf ci_system-test-workflow_C
# rm -rf ci_system-test-workflow_D
#
# rm -rf sag_system-test-workflow
# rm -rf sag_system-test-offsite
#
# # Workflow selection

workflows_abel = ["system-test-workflow_A.yml"
"system-test-_abelworkflow_B.yml",
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

def download(workflows, repo, folders):
    # Download and unzip results from test workflows

    if repo == "abelikt":
        prefix= "ci_"
    elif repo =="thin-edge":
        prefix="sag_"

    for w in workflows:
        y = w.replace(".zip",".yml")
        print(w, y)
        cmd=f"./download_workflow_artifact.py {repo} {w} -o ci_{w};"
        print(cmd)
        os.system(cmd)
        cmd =f"unzip -q -o -d ci_{y} ci_{y};"
        print(cmd)
        os.system(cmd)

    # Doublecheck if our result folders are there

    for f in folders:
        print(f)
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
                files += f"{prefix}/PySys/pysys_junit_xml_{tag}/*.xml"

        cmd = f"junitparser merge {files} { report }.xml"

        print(cmd)
        #os.system(cmd)
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
    os.system(cmd)

    cmd = f"junit2html --summary-matrix {XMLFILES} > report.out"
    print(cmd)
    os.system(cmd)

    # # Build report matrix
    cmd = f"junit2html --report-matrix report-matrix.html {XMLFILES}"
    print(cmd)
    os.system(cmd)

    # Zip everything
    # zip report.zip *.html *.json


#download( workflows_abel, "abelikt", folders_abel)
#download( workflows_sag, "thin-edge", folders_sag)

runners = {
        "michael":{     "prefix":"ci_system-test-workflow",   "report":"ci_system-test-report",  "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
        "offsitea":{    "prefix":"ci_system-test-workflow_A", "report":"ci_system-test-report",  "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
        "offsiteb":{    "prefix":"ci_system-test-workflow_B", "report":"ci_system-test-report",  "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
        "offsitec":{    "prefix":"ci_system-test-workflow_C", "report":"ci_system-test-report",  "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
        "offsited":{    "prefix":"ci_system-test-workflow_D", "report":"ci_system-test-report",   "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },

        "sag":{         "prefix":"sag_system-test-workflow",  "report":"sag_system-test-report_workflow",  "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
        "offsite-sag":{ "prefix":"sag_system-test-offsite",   "report":"sag_system-test-report_offsite",   "tests":["all", "apt", "apama", "docker", "sm", "analytics"] },
            }

print(runners.keys())

for key in runners.keys():
    postprocess_runner( runners[key] )

postprocess()
