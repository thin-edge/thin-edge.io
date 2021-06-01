
# Postprocessing is currently intended to be executed on a self-
# hosted runner due to all the network accesses.
set -e

# Enable the Big Query environment
source ~/env-bigquery/bin/activate

# Run the tests for postprocessing
# Not ideal here, but for now, the environment seems to be needed
# so that the google module is available
python3 -m pytest ci/statistics

# Download all the artifacts
./ci/statistics/download_all_artifacts.py abelikt

# Trigger postprocessing and upload
# For a yet unkown reason we need to call this via the python
# interpreter. Otherwise the google module is not found
python3 ./ci/statistics/process_lake.py google


