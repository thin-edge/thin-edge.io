#!/bin/bash

# Call this file to generate a report locally

set -e

python3 -m venv ~/env-builder
source ~/env-builder/bin/activate
pip3 install junitparser
pip3 install junit2html

mkdir -p ./ci/report
cd ./ci/report

./report_builder.py abelikt ci_pipeline.yml --download

