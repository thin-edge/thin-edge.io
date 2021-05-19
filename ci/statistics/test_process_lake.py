
# Hint install pytest in the google environment,
# otherwise it will watch for the google modules
# outside of the envionment.

# Workaround:
# python -m pytest test_process_lake.py

import numpy as np
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


def test_postprocess_vals():
    """Tightnen current functionality for now
    Probably too much for a simple test
    """
    lake = os.path.expanduser( '~/DataLakeTest')
    cpuidx = 0
    memidx = 0
    relevant_measurement_folders = [
        'results_1_unpack',
        'results_2_unpack',
        'results_4_unpack'
        ]
    data_length = 10
    cpu_array = pl.CpuHistory( len(relevant_measurement_folders)*data_length )
    mem_array = pl.MemoryHistory( len(relevant_measurement_folders)*data_length )
    cpu_hist_array = pl.CpuHistoryStacked ( data_length )

    pl.postprocess_vals(  data_length, relevant_measurement_folders,
    cpu_array, mem_array, cpuidx, memidx, cpu_hist_array)

    for i in cpu_array.array:
        print(i)

    #programmatically reproduce the data set
    data = []
    for i in range( len(relevant_measurement_folders) * data_length):
        if i < 20:
            data.append( [i, i//10, i, i+1, i+2, 0, 0] )
        else:
            data.append( [i, 4, i, i+1, i+2, 0, 0] )


    exp=np.array( [
        data
 ], dtype=np.int32)

    assert exp.all() == cpu_array.array.all()





