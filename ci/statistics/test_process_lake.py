
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


def test_postprocess_vals_cpu():
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

    #programmatically reproduce the data set
    data = []
    for i in range( len(relevant_measurement_folders) * data_length):
        if i < 20:
            k=  (i+10)//10
        else:
            k = 4

        data.append( [i, k, i%10, i+1, i+2, 0, 0] )

    exp=np.array( data , dtype=np.int32)

    extensive_check = False
    if extensive_check:
        print("Expect")
        print(exp)
        print('There')
        print(cpu_array.array)

        for i in range(len(data)):
            print("Line", i, np.array_equal(exp[i], cpu_array.array[i]) )

    assert np.array_equal(exp, cpu_array.array)

def test_postprocess_vals_mem():
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

    #programmatically reproduce the data set
    data = []
    for i in range(len(relevant_measurement_folders) * data_length):
        if i < 20:
            k=(i+10)//10
        else:
            k=4
        data.append( [i, k, i%10, 100+i, 200+i, 300+i, 400+i, 500+i] )

    exp=np.array( data, dtype=np.int32)

    extensive_check = False
    if extensive_check:
        print("Expect")
        print(exp)
        print('There')
        print(mem_array.array)

        for i in range(len(data)):
            print("Line", i, np.array_equal(exp[i], mem_array.array[i]) )

    assert np.array_equal(exp, mem_array.array)
