#!/bin/bash

set -e

python3 -m venv ~/env-pysys
source ~/env-pysys/bin/activate
pip3 install -r tests/requirements.txt

cd ci/report/

# Cleanup

rm -f *.zip
rm -f *.xml
rm -f *.html
rm -f *.json
rm -rf system-test-workflow
rm -rf system-test-workflow_A
rm -rf system-test-workflow_B
rm -rf system-test-workflow_C
rm -rf system-test-workflow_D

rm -rf ci_system-test-workflow
rm -rf ci_system-test-workflow_A
rm -rf ci_system-test-workflow_B
rm -rf ci_system-test-workflow_C
rm -rf ci_system-test-workflow_D

rm -rf sag_system-test-workflow
rm -rf sag_system-test-offsite

# Workflow selection

# Download and unzip results from test workflows

WORKFLOWS="system-test-workflow.yml"
WORKFLOWS+=" system-test-offsite.yml"

for i in $WORKFLOWS;
    do
    echo $i;
    ./download_workflow_artifact.py thin-edge $i -o sag_$i;
    unzip -q -o -d sag_${i/.yml/} sag_${i/.yml/.zip};
done


# Postprocess results
source ~/env-pysys/bin/activate

SAGOUT="sag_system-test-report"

for X in "workflow"
    do
    echo "Processing: $X"
    FILES="sag_system-test-$X/PySys/pysys_junit_xml_all/*.xml"
    junitparser merge $FILES $SAGOUT"_"$X.xml
    junit2html $SAGOUT"_"$X.xml
done

for X in "offsite"
    do
    echo "Processing: $X"
    FILES="sag_system-test-$X/PySys/pysys_junit_xml_all/*.xml"
    FILES+=" sag_system-test-$X/PySys/pysys_junit_xml_apt/*.xml"
    FILES+=" sag_system-test-$X/PySys/pysys_junit_xml_docker/*.xml"
    FILES+=" sag_system-test-$X/PySys/pysys_junit_xml_sm/*.xml"
    junitparser merge $FILES $SAGOUT"_"$X.xml
    junit2html $SAGOUT"_"$X.xml
done

XMLFILES=$SAGOUT"_offsite.xml "$SAGOUT"_workflow.xml"

echo "XML files to process:" $XMLFILES

# Print summary matrix
junit2html --summary-matrix $XMLFILES

# Build report matrix
junit2html --report-matrix report-matrix.html $XMLFILES

# Zip everything
zip report.zip *.html *.json

deactivate

