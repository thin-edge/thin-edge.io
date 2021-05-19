
# Hint install pytest in the google environment,
# otherwise it will watch for the google modules
# outside of the envionment.

# Workaround:
# python -m pytest test_process_lake.py

import os
from os.path import expanduser
from pathlib import Path


import process_lake as pl

def test_get_measurement_foders():
    """Still clumsy check"""

    lake = os.path.expanduser( '~/DataLakeTest' )
    ret = pl.get_measurement_folders(lake)
    exp = [
        'results_1_unpack',
        'results_2_unpack',
        'results_4_unpack'
        ]
    assert ret == exp