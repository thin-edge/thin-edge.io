#!/bin/bash

# Call this file to generate a report from a GitHub Workflow

set -e

python3 -m venv ~/env-builder
source ~/env-builder/bin/activate
pip3 install junitparser
pip3 install junit2html

./ci/report/report_builder.py --folder ./results thin-edge ci_pipeline.yml
