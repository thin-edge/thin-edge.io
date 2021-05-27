
set -e

source ~/env-bigquery/bin/activate

./cistatistics/download_all_artifacts.py

python3 ./ci/statistics/process_lake.py google


