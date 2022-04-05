#!/bin/bash

set -e

python3 -m venv ~/env-pysys
source ~/env-pysys/bin/activate
pip3 install -r tests/requirements.txt

cd ci/report/

# ./report_builder.py abelikt commit-workflow-allinone.yml --download

./report_builder.py abelikt commit-workflow-allinone.yml
