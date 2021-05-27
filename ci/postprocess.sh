
set -e

source ~/env-bigquery/bin/activate

statistics/download_all_artifacts.py

python3 ./statistics/process_lake.py google


