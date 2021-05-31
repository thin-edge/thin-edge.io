
set -e

source ~/env-bigquery/bin/activate

./ci/statistics/download_all_artifacts.py

python3 ./ci/statistics/process_lake.py google


