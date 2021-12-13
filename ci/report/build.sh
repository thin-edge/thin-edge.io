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

# Workflow selection

WORKFLOWS="system-test-workflow_A.yml"
#WORKFLOWS+=" system-test-workflow_Azure.yml"
WORKFLOWS+=" system-test-workflow_B.yml"
WORKFLOWS+=" system-test-workflow_C.yml"
WORKFLOWS+=" system-test-workflow_D.yml"
WORKFLOWS+=" system-test-workflow.yml"

# Download and unzip results from test workflows

for i in $WORKFLOWS;
    do
    echo $i;
    ./download_workflow_artifact.py abelikt $i -o $i;
    unzip -q -o -d ${i/.yml/} ${i/.yml/.zip};
done

# Postprocess results

source ~/env-pysys/bin/activate

OUT="system-test-report"

for X in "" "_A" "_B" "_C" "_D"
    do
    echo "Processing: $X"
    FILES="system-test-workflow$X/PySys/pysys_junit_xml_all/*.xml"
    FILES+=" system-test-workflow$X/PySys/pysys_junit_xml_apt/*.xml"
    FILES+=" system-test-workflow$X/PySys/pysys_junit_xml_docker/*.xml"
    FILES+=" system-test-workflow$X/PySys/pysys_junit_xml_sm/*.xml"
    junitparser merge $FILES $OUT$X.xml
    junit2html $OUT$X.xml
done

# Print summary matrix

junit2html --summary-matrix "$OUT".xml "$OUT"_A.xml "$OUT"_B.xml "$OUT"_C.xml "$OUT"_D.xml

# Build report matrix

junit2html --report-matrix report-matrix.html "$OUT".xml "$OUT"_A.xml "$OUT"_B.xml "$OUT"_C.xml "$OUT"_D.xml

# Zip everything

zip report.zip "$OUT"-matrix.html "$OUT".xml.html "$OUT"_A.xml.html "$OUT"_B.xml.html "$OUT"_C.xml.html "$OUT"_D.xml.html *.json

deactivate

