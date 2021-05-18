
# source ~/env-bigquery/bin/activate
# export GOOGLE_APPLICATION_CREDENTIALS="/home/micha/Project-SAG/Statistics/sturdy-mechanic-312713-14b2e55c4ad0.json"

# WT
# Error {'reason': 'quotaExceeded', 'location': 'max_dml_outstanding_per_table', 'message': 'Quota exceeded: Your table exceeded quota for total number of dml jobs writing to a table, pending + running. For more information, see https://cloud.google.com/bigquery/troubleshooting-errors'}


import sys
import time
import logging
import json
import subprocess
import os
import os.path
from pathlib import Path
import numpy as np
from numpy.core.records import array

lake = os.path.expanduser( '~/DataLake' )

logging.basicConfig(level=logging.INFO)

style = 'google'  #'ms', 'google', 'none'


if style == 'ms':
    server = 'mysupertestserver.database.windows.net'
    database = 'mysupertestdatabase'
    username = 'mysupertestadmin'
    password = 'not_here'

    import pymssql
    conn = pymssql.connect(server, username, password, database)
    client = conn.cursor(as_dict=True)
    dbo = 'dbo'
    integer = 'INTEGER'
    simulate = False

elif style == 'google':
    sleep = 0.2

    from google.cloud import bigquery
    #from google.api_core.exceptions import NotFound

    client = bigquery.Client()
    dbo = 'ADataSet'
    integer = 'INT64'
    simulate = False

elif style == 'none':
    simulate = True
    dbo = 'Nopdb'
    integer = 'Nopint'
    client = None

else:
    sys.exit(1)

cpu_table = 'ci_cpu_measurement_tedge_mapper'
mem_table = 'ci_mem_measurement_tedge_mapper'
cpu_hist_table = 'ci_cpu_hist'

create_cpu = f"""
CREATE TABLE {dbo}.{cpu_table} (
id {integer},
mid {integer},
sample {integer},
utime                    {integer},
stime                    {integer},
cutime                   {integer},
cstime                   {integer}
);
"""

create_mem=f"""
CREATE TABLE {dbo}.{mem_table} (
id {integer},
mid {integer},
sample {integer},
size {integer},
resident {integer},
shared {integer},
text {integer},
data {integer}
);
"""

# insert into mytable values ( 0, 0, 1,2,3,4 );

create_cpu_hist = f"""
CREATE TABLE {dbo}.{cpu_hist_table} (
id {integer},
last                    {integer},
old                     {integer},
older                   {integer},
evenolder                   {integer},
evenmovreolder                   {integer}
);
"""

class CpuHistory:
    """Mostly the representation of a unpublished SQL table"""
    def __init__(self, size):
        self.array = np.zeros((size, 7), dtype=np.int32)
        self.size = size
        self.name = 'ci_cpu_measurement_tedge_mapper'
        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"

    def insert_line(self, idx, mid, sample, utime, stime, cutime, cstime):
        self.array[idx] = [idx, mid, sample, utime, stime, cutime, cstime]

    def show(self):
        import matplotlib.pyplot as plt
        fig, ax = plt.subplots()

        #ax.plot(self.array[:,0], 'o-')
        ax.plot(self.array[:, 1], '.', label='mid')
        ax.plot(self.array[:, 2], '-', label='sample')
        ax.plot(self.array[:, 3], '-', label='utime')
        ax.plot(self.array[:, 4], '-', label='stime')
        ax.plot(self.array[:, 5], '-', label='cutime')
        ax.plot(self.array[:, 6], '-', label='cstime')
        plt.legend()
        plt.title('CPU History')

        plt.show()

    def delete_table(self):
        try:
            client.delete_table( self.database)
        except:# google.api_core.exceptions.NotFound:
            pass

    def update_table(self):
            print("Updating table:", self.name)
            job_config = bigquery.LoadJobConfig(
                schema=[
                    bigquery.SchemaField("id", "INT64"),
                    bigquery.SchemaField("mid", "INT64"),
                    bigquery.SchemaField("sample", "INT64"),
                    bigquery.SchemaField("utime", "INT64"),
                    bigquery.SchemaField("stime", "INT64"),
                    bigquery.SchemaField("cutime", "INT64"),
                    bigquery.SchemaField("cstime", "INT64"),
                ],
            )

            data = []

            for i in range(self.size):
                data.append(
                    {
                    "id":int(self.array[i,0]), "mid":int(self.array[i,1]),
                    "sample":int(self.array[i,2]), "utime":int(self.array[i,3]),
                    "stime":int(self.array[i,4]), "cutime":int(self.array[i,5]),
                    "cstime":int(self.array[i,6]) } )

            load_job = client.load_table_from_json( data,
                self.database,
                job_config=job_config)

            while load_job.running():
                time.sleep(0.5)
                print('Waiting')

            if load_job.errors:
                print("Error", load_job.error_result)
                print(load_job.errors)
                sys.exit(1)

class CpuHistoryStacked:
    """Mostly the representation of a unpublished SQL table"""

    def __init__(self, size):
        self.size = size
        self.name = 'ci_cpu_hist'
        self.fields = [
            ("id", "INT64"),
            ("t0u", "INT64"),
            ("t0s", "INT64"),
            ("t1u", "INT64"),
            ("t1s", "INT64"),
            ("t2u", "INT64"),
            ("t2s", "INT64"),
            ("t3u", "INT64"),
            ("t3s", "INT64"),
            ("t4u", "INT64"),
            ("t4s", "INT64"),
            ("t5u", "INT64"),
            ("t5s", "INT64"),
            ("t6u", "INT64"),
            ("t6s", "INT64"),
            ("t7u", "INT64"),
            ("t7s", "INT64"),
            ("t8u", "INT64"),
            ("t8s", "INT64"),
            ("t9u", "INT64"),
            ("t9s", "INT64")]
        self.array = np.zeros((size, len(self.fields)), dtype=np.int32)


    def insert_line(self,line):
        assert len(line)==len(self.fields)
        self.array[idx] = line

    def show(self):
        import matplotlib.pyplot as plt
        fig, ax = plt.subplots()

        for i in range(len(self.fields)):
            if i%2==0:
                style ='-o'
            else:
                style = '-x'
            ax.plot(self.array[:, i], style, label=self.fields[i][0])

        plt.legend()
        plt.title('CPU History Stacked')

        plt.show()

    def delete_table(self):
        try:
            client.delete_table( f"sturdy-mechanic-312713.ADataSet.{self.name}")
        except:# google.api_core.exceptions.NotFound:
            pass

    def update_table(self):
            print("Updating table:", self.name)
            schema = []
            for i in range(len(self.fields)):
                schema.append(bigquery.SchemaField(self.fields[i][0], self.fields[i][1]))

            job_config = bigquery.LoadJobConfig(
                schema=schema
            )

            data = []

            for i in range(self.size):
                line={}
                for j in range(len(self.fields)):
                    line[ self.fields[j][0] ] = int(self.array[i,j])
                data.append( line )

            load_job = client.load_table_from_json( data,
                f"sturdy-mechanic-312713.ADataSet.{self.name}",
                job_config=job_config)

            while load_job.running():
                time.sleep(0.5)
                print('Waiting')

            if load_job.errors:
                print("Error", load_job.error_result)
                print(load_job.errors)
                sys.exit(1)

class MemoryHistory:
    def __init__(self, size):
        self.array = np.zeros((size, 8), dtype=np.int32)
        self.size = size
        self.name = 'ci_mem_measurement_tedge_mapper'
        self.database=f"sturdy-mechanic-312713.ADataSet.{self.name}"

    def insert_line(self, idx, mid, sample, size, resident, shared, text, data):
        self.array[idx] = [idx, mid, sample, size, resident, shared, text, data]

    def show(self):
        import matplotlib.pyplot as plt
        fig, ax = plt.subplots()
        style = '.'
        #ax.plot(self.array[:,0], 'o-')
        ax.plot(self.array[:, 1], style, label='mid')
        ax.plot(self.array[:, 2], style, label='sample')
        ax.plot(self.array[:, 3], style, label='size')
        ax.plot(self.array[:, 4], style, label='resident')
        ax.plot(self.array[:, 5], style, label='shared')
        ax.plot(self.array[:, 6], style, label='text')
        ax.plot(self.array[:, 7], style, label='data')

        plt.legend()
        plt.title('Memory History')

        plt.show()

    def delete_table(self):
        try:
            client.delete_table( self.database )
        except:# google.api_core.exceptions.NotFound:
            pass

    def update_table_one_by_one(self):
        for i in range(self.size):
            assert self.array[i,0] == i
            q= f"insert into {dbo}.{mem_table} values ( {i}, {self.array[i,1]}," \
            f" {self.array[i,2]}, {self.array[i,3]},{self.array[i,4]}," \
            f"{self.array[i,5]},{self.array[i,6]}, {self.array[i,7]} );"
            #print(q)
            myquery( client, q)

    def update_table(self):
            print("Updating table:", self.name)
            job_config = bigquery.LoadJobConfig(
                schema=[
                    bigquery.SchemaField("id", "INT64"),
                    bigquery.SchemaField("mid", "INT64"),
                    bigquery.SchemaField("sample", "INT64"),
                    bigquery.SchemaField("size", "INT64"),
                    bigquery.SchemaField("resident", "INT64"),
                    bigquery.SchemaField("shared", "INT64"),
                    bigquery.SchemaField("text", "INT64"),
                    bigquery.SchemaField("data", "INT64"),
                ],
            )

            data = []

            for i in range(self.size):
                data.append(
                    {
                    "id":int(self.array[i,0]), "mid":int(self.array[i,1]),
                    "sample":int(self.array[i,2]), "size":int(self.array[i,3]),
                    "resident":int(self.array[i,4]), "shared":int(self.array[i,5]),
                    "text":int(self.array[i,6]), "data":int(self.array[i,6]) } )

            load_job = client.load_table_from_json( data,
                self.database,
                job_config=job_config)

            while load_job.running():
                time.sleep(0.5)
                print('Waiting')

            if load_job.errors:
                print("Error", load_job.error_result)
                print(load_job.errors)
                sys.exit(1)


def scrap_mem(thefile, mesaurement_index, client, dbo, memidx, arr):
    with open(thefile) as thestats:
        lines = thestats.readlines()
        sample  = 0
        for line in lines:
            #print(line)
            entries = line.split()
            #print(entries)
            size  = entries[1-1] #     (1) total program size
            resident= entries[2-1] #   (2) resident set size
            shared= entries[3-1] #     (3) number of resident shared pages
            text= entries[4-1] #       (4) text (code)
            #lib = entries[5-1] #      (5) library (unused since Linux 2.6; always 0)
            data = entries[6-1] #      (6) data + stack

            arr.insert_line(idx=memidx, mid=mesaurement_index, sample=sample, size=size, resident=resident, shared=shared, text=text, data=data)
            sample += 1
            memidx += 1

    logging.debug(f"Read {sample} Memory stats")
    missing = 60 - sample
    for m in range(missing):

        arr.insert_line(idx=memidx, mid=mesaurement_index, sample=sample, size=0, resident=0, shared=0,
                        text=0, data=0)
        sample += 1
        memidx += 1

    return memidx

def scrap_cpu(thefile, mesaurement_index, client,dbo, cpuidx, arr):

    with open(thefile) as thestats:
        lines = thestats.readlines()
        sample  = 0

        for line in lines:
            #print(line)
            entries = line.split()
            #print(len(entries))
            if len(entries) == 52 and entries[1]=='(tedge_mapper)':
                ut = int(entries[14-1])
                st = int(entries[15-1])
                ct = int(entries[16-1])
                cs = int(entries[17-1])
                #print(idx, ut,st,ct,cs)

                arr.insert_line( idx=cpuidx, mid=mesaurement_index, sample=sample, utime=ut, stime=st, cutime=ct, cstime=cs )
                sample += 1
                cpuidx += 1

    logging.debug(f"Read {sample} cpu stats")
    missing = 60 - sample
    for m in range(missing):
        arr.insert_line(idx=cpuidx, mid=mesaurement_index, sample=sample, utime=0, stime=0, cutime=0, cstime=0)
        sample += 1
        cpuidx += 1

    return cpuidx

def myquery(client, query):

    logging.info(query)

    if style == 'ms':
        client.execute( query )
        global conn
        conn.commit()

    elif style == 'google':

        query_job = client.query( query )
        if query_job.errors:
            print("Error", query_job.error_result)
            sys.exit(1)
        time.sleep(sleep)
    elif style == 'none':
        pass
    else:
        sys.exit(1)

def postprocess_vals(measurement_folders, cpu_array, mem_array, cpuidx, memidx, cpu_hist_array):

    for folder in measurement_folders:
        mesaurement_index = int(folder.split('_')[1].split('.')[0])

        statsfile = f"{lake}/{folder}/PySys/publish_sawmill_record_statistics/Output/linux/stat_mapper_stdout.out"
        cpuidx = scrap_cpu(statsfile, mesaurement_index, client, dbo, cpuidx, cpu_array)

        statsfile = f"{lake}/{folder}/PySys/publish_sawmill_record_statistics/Output/linux/statm_mapper_stdout.out"
        memidx = scrap_mem(statsfile, mesaurement_index, client, dbo, memidx, mem_array)

#    for folder in measurement_folders[-10:]:
    mlen = len(measurement_folders)

    for i in range(60):
        cpu_hist_array.array[i, 0] = i

    column = 1
    for m in range(mlen-1, mlen-10-1, -1 ):
        #print(m)
        for i in range(60):
            #print( cpu_array.array[ m*60+i ,3],  cpu_array.array[ m*60+i ,4] )
            cpu_hist_array.array[i, column] = cpu_array.array[ m*60+i ,3]
            cpu_hist_array.array[i, column+1] = cpu_array.array[ m*60+i ,4]
        column += 2

    #print( cpu_hist_array.array )


def unzip_results():
    p = Path(lake)
    for child in p.iterdir():
        if child.is_dir():
            logging.debug (child.name)
            pass
        elif child.name.endswith('.zip'):
            logging.debug(child.name)
            new_name = child.name.removesuffix('.zip')
            new_folder = f"{new_name}_unpack/"
            if not os.path.exists( os.path.join(lake,new_folder)):
                proc = subprocess.run(["unzip", child.name, "-d", new_folder], cwd=lake)

def generate():
    logging.info("Unzip Results")
    unzip_results()
    logging.info("Sumarize List")

    s = sorted(Path(lake).glob('*_unpack'), key=lambda _: int(_.name.split('_')[1].split('.')[0]))

    measurement_folders = []
    for y in s:
        logging.debug(y.name)
        measurement_folders.append(y.name)

    task = 'generate'
    if task == 'generate':
        # overall row index for the cpu table
        cpuidx = 0
        # overall row index for the memory table
        memidx = 0

        #myquery( client, f"drop table {dbo}.{cpu_table}" )
        #myquery( client, f"drop table {dbo}.{mem_table}" )
        #myquery( client, f"drop table {dbo}.{cpu_hist_table}" )

        #myquery(client, create_mem)
        #myquery(client, create_cpu)
        #myquery(client, create_cpu_hist)

        #print(measurement_folders)

        # last earliest valid is 'results_107_unpack'
        max_processing_range=23
        earliest_valid = 'results_107_unpack'

        #print(measurement_folders[-max_processing_range])

        assert measurement_folders[-max_processing_range] == earliest_valid
        processing_range = max_processing_range

    elif task == 'update':
        # overall row index for the cpu table
        cpuidx =1164+1 #0
        # overall row index for the memory table
        memidx =1164+1 #0

        processing_range = 1
        last_valid = 'results_138_unpack'
        assert measurement_folders[-processing_range] == last_valid


    relevant_measurement_folders = measurement_folders[-processing_range:]

    logging.info('Procesing Range ' + str( len(relevant_measurement_folders[-processing_range:])))
    logging.info('Procesing Build Numbers: ')
    for m in relevant_measurement_folders[-processing_range:]:
        print(m.split('_')[1], end=" ")
    print("")

    logging.info("Postprocessing")

    cpu_array = CpuHistory( len(relevant_measurement_folders)*60 )
    mem_array = MemoryHistory( len(relevant_measurement_folders)*60 )
    cpu_hist_array = CpuHistoryStacked ( 60 )

    postprocess_vals(  relevant_measurement_folders, cpu_array, mem_array, cpuidx, memidx , cpu_hist_array)

    cpu_array.show()
    mem_array.show()
    cpu_hist_array.show()

    #sys.exit(1)

    logging.info("Uploading")

    cpu_array.delete_table()
    cpu_array.update_table()
    mem_array.delete_table()
    mem_array.update_table()
    cpu_hist_array.delete_table()
    cpu_hist_array.update_table()

    logging.info("Done")

if __name__ == '__main__':
    generate()