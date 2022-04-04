#!/bin/bash

# This solution is far from perfect
# TODO Make it more flexible
# TODO Make it more obvious what is going on here
# TODO Host this report somewhere
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

WORKFLOWS = "system-test-workflow_A.yml"
# WORKFLOWS+=" system-test-workflow_Azure.yml"
WORKFLOWS += " system-test-workflow_B.yml"
WORKFLOWS += " system-test-workflow_C.yml"
WORKFLOWS += " system-test-workflow_D.yml"
WORKFLOWS += " system-test-workflow.yml"


def download():
    # Download and unzip results from test workflows

    # for i in $WORKFLOWS;
    #     do
    #     echo $i;
    #     ./download_workflow_artifact.py abelikt $i -o ci_$i;
    #     unzip -q -o -d ci_${i/.yml/} ci_${i/.yml/.zip};
    # done

    # # Doublecheck if our result folders are there
    # FOLDERS="ci_system-test-workflow ci_system-test-workflow_A ci_system-test-workflow_B ci_system-test-workflow_C ci_system-test-workflow_D"
    # for FOLDER in $FOLDERS; do
    #     if [ ! -d $FOLDER ]; then
    #         echo "Folder missing: " $FOLDER
    #     fi
    # done

    # # Workflow selection for official repository

    WORKFLOWS = "system-test-workflow.yml"
    WORKFLOWS += " system-test-offsite.yml"

    # for i in $WORKFLOWS;
    #     do
    #     echo $i;
    #     ./download_workflow_artifact.py thin-edge $i -o sag_$i;
    #     unzip -q -o -d sag_${i/.yml/} sag_${i/.yml/.zip};
    # done

    # # Doublecheck if our result folders are there
    # FOLDERS="sag_system-test-workflow sag_system-test-offsite"
    # for FOLDER in $FOLDERS; do
    #     if [ ! -d $FOLDER ]; then
    #         echo "Folder missing: " $FOLDER
    #     fi
    # done


def postprocess():
    # Postprocess results
    OUT = "ci_system-test-report"

    # Postporcess results for the onsite runner onsite at Michael

    for X in ["empty"]:
        X = ""
        print(f"Processing: {X}")
        FILES = f"ci_system-test-workflow{X}/PySys/pysys_junit_xml_all/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_apt/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_apama/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_docker/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_sm/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_analytics/*.xml"
        cmd = f"junitparser merge {FILES} {OUT}{X}.xml"
        print(cmd)
        os.system(cmd)
        print(f"junit2html {OUT}{X}.xml")

    # Postporcess results for the offsite runners from Michael
    for X in ["_A", "_B", "_C", "_D"]:
        print(f"Processing: {X}")
        FILES = f"ci_system-test-workflow{X}/PySys/pysys_junit_xml_all/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_apt/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_apama/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_docker/*.xml"
        FILES += f" ci_system-test-workflow{X}/PySys/pysys_junit_xml_sm/*.xml"
        cmd = f"junitparser merge {FILES} {OUT}{X}.xml"
        print(cmd)
        os.system(cmd)
        print(f"junit2html {OUT}{X}.xml")

    SAGOUT = "sag_system-test-report"

    # Postporcess results for the local runner onsite at Rina
    for X in ["workflow"]:
        print(f"Processing: {X}")
        FILES = f"sag_system-test-{X}/PySys/pysys_junit_xml_all/*.xml"
        cmd = f"junitparser merge {FILES} {SAGOUT}_{X}.xml"
        print(cmd)
        os.system(cmd)
        print(f"junit2html {SAGOUT}_{X}.xml")

    # Postporcess results for the official runners offsite
    for X in ["offsite"]:
        print(f"Processing: {X}")
        FILES = f"sag_system-test-{X}/PySys/pysys_junit_xml_all/*.xml"
        FILES += f" sag_system-test-{X}/PySys/pysys_junit_xml_apt/*.xml"
        # FILES+=" sag_system-test-$X/PySys/pysys_junit_xml_apama/*.xml"
        FILES += f" sag_system-test-{X}/PySys/pysys_junit_xml_docker/*.xml"
        FILES += f" sag_system-test-{X}/PySys/pysys_junit_xml_sm/*.xml"
        cmd = f"junitparser merge {FILES} {SAGOUT}_{X}.xml"
        print(cmd)
        os.system(cmd)
        print(" junit2html {SAGOUT}_{X}.xml")

    # Create a combined report matrix from all report sources

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


download()
# postprocess()
