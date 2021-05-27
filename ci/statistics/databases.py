"""Database classes and support for measurements"""

from abc import ABC
import json
import logging
import os
import re
import sys
import time
import numpy as np
from typing import Tuple

from google.cloud import bigquery

logging.basicConfig(level=logging.INFO)

def get_database(style: str):
    """Retrive database to the database to be used
    Returns database client, database object, Integer type to be used
    and a connection object (for Azure)
    """

    conn = None
    if style == "ms":
        logging.info("Using Microsoft Azure backend")
        server = "mysupertestserver.database.windows.net"
        database = "mysupertestdatabase"
        username = "mysupertestadmin"
        password = "not_here"

        import pymssql

        conn = pymssql.connect(server, username, password, database)
        client = conn.cursor(as_dict=True)
        dbo = "dbo"
        integer = "INTEGER"

    elif style == "google":
        logging.info("Using Google Big Query backend")
        client = bigquery.Client()
        dbo = "ADataSet"
        integer = "INT64"

    elif style == "none":
        logging.info("Using No Backend")
        dbo = "Nopdb"
        integer = "Nopint"
        client = None

    else:
        logging.error("Error in configuring database")
        sys.exit(1)

    return client, dbo, integer, conn


# cpu_table = "ci_cpu_measurement_tedge_mapper"
# mem_table = "ci_mem_measurement_tedge_mapper"
# cpu_hist_table = "ci_cpu_hist"


# def myquery(client, query, conn, style):
#
#    logging.info(query)
#
#    if style == "ms":
#        client.execute(query)
#        conn.commit()
#
#    elif style == "google":
#
#        query_job = client.query(query)
#        if query_job.errors:
#            logging.error("Error", query_job.error_result)
#            sys.exit(1)
#        time.sleep(0.3)
#    elif style == "none":
#        pass
#    else:
#        sys.exit(1)


# def get_sql_create_cpu_table(dbo, name, integer):
#     create_cpu = f"""
#     CREATE TABLE {dbo}.{name} (
#     id {integer},
#     mid {integer},
#     sample {integer},
#     utime                    {integer},
#     stime                    {integer},
#     cutime                   {integer},
#     cstime                   {integer}
#     );
#     """


# def get_sql_create_mem_table(dbo, name, integer):

#     create_mem = f"""
#     CREATE TABLE {dbo}.{name} (
#     id {integer},
#     mid {integer},
#     sample {integer},
#     size {integer},
#     resident {integer},
#     shared {integer},
#     text {integer},
#     data {integer}
#     );
#     """


# def get_sql_create_mem_table(dbo, name, integer):
#     create_cpu_hist = f"""
#     CREATE TABLE {dbo}.{name} (
#     id {integer},
#     last                    {integer},
#     old                     {integer},
#     older                   {integer},
#     evenolder                   {integer},
#     evenmovreolder                   {integer}
#     );
#     """


# def get_sql_create_mem_table(dbo, name, client):
#     myquery(client, f"drop table {dbo}.{cpu_table}")


# def get_sql_create_mem_table(dbo, name, client):
#     myquery(client, f"drop table {dbo}.{mem_table}")


# def get_sql_create_mem_table(dbo, name, client):
#     myquery(client, f"drop table {dbo}.{cpu_hist_table}")


class MeasurementBase(ABC):
    """Abstract base class for type Measurements
    """

    def postprocess():
        """Postprocess all relevant folders
        """
        pass

    def show():
        """Show content on console
        """
        pass

    def update_table(self):
        """Create table and prepare loading via json and upload
        """
        pass

    def delete_table(self):
        try:
            self.client.delete_table(self.database)
        except:  # TODO: Can' import this google.api_core.exceptions.NotFound:
            pass

    def upload_table(self):
        """Upload table to online database
        """

        if self.client:
            load_job = self.client.load_table_from_json(
                self.json_data,
                self.database,
                job_config=self.job_config,
            )

            while load_job.running():
                time.sleep(0.5)
                logging.info("Waiting")

            if load_job.errors:
                logging.error(f"Error {load_job.error_result}")
                logging.error(load_job.errors)
                raise SystemError

    @staticmethod
    def foldername_to_index(foldername: str) -> int:
        """Convert results_N_unpack into N
        """
        reg = re.compile(r"^results_(\d+)_unpack$")
        match = reg.match(foldername)

        if match:
            index = int( match.group(1) )
        else:
            raise SystemError("Cannot convert foldername")

        return index


class MeasurementMetadata(MeasurementBase):
    """Class to represent a table of measurement metadata
    """

    def __init__(self, size, client, testmode, lake):
        self.array = []
        self.client = client
        self.size = size
        if testmode:
            self.name = "ci_measurements_test"
        else:
            self.name = "ci_measurements"
        self.lake = lake

        # TODO move to baseclass
        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"

    def scrap_measurement_metadata(self, file: str) -> Tuple[str, str, str, str, str]:
        """Read measurement data from file
        """

        with open(file) as content:
            data = json.load(content)
            run = data["run_number"]
            date = data["updated_at"]
            url = data["html_url"]
            name = data["name"]
            branch = data["head_branch"]

        return run, date, url, name, branch


    def postprocess(self, folders):
        """Postprocess all relevant folders
        """
        idx = 0
        for folder in folders:
            index = self.foldername_to_index(folder)

            name = f"system_test_{index}_metadata.json"
            path = os.path.join(self.lake, name)

            run, date, url, name, branch = self.scrap_measurement_metadata(path)

            self.array.append((idx, run, date, url, name, branch))
            idx += 1

        return self.array

    def show(self):
        """Show content on console
        """
        logging.info(f"Content of table {self.database}")
        for row in self.array:
            logging.info(row)

    def update_table(self):
        """Create table and prepare loading via json and upload
        """

        logging.info("Updating table:" + self.name)

        self.delete_table()

        self.job_config = bigquery.LoadJobConfig(
            schema=[
                bigquery.SchemaField("id", "INT64"),
                bigquery.SchemaField("mid", "INT64"),
                bigquery.SchemaField("date", "STRING"),
                bigquery.SchemaField("url", "STRING"),
                bigquery.SchemaField("name", "STRING"),
                bigquery.SchemaField("branch", "STRING"),
            ],
        )

        self.json_data = []

        for index in range(self.size):
            self.json_data.append(
                {
                    "id": self.array[index][0],
                    "mid": self.array[index][1],
                    "date": self.array[index][2],
                    "url": self.array[index][3],
                    "name": self.array[index][4],
                    "branch": self.array[index][5],
                }
            )

        self.upload_table()

class CpuHistory(MeasurementBase):
    """Class to represent a table of measured CPU usage
    """

    def __init__(self, name, lake, size, data_length, client, testmode):
        self.array = np.zeros((size, 7), dtype=np.int32)
        self.size = size
        self.data_length = data_length
        self.client = client
        self.lake = lake
        if testmode:
            self.name = name + "_test"
        else:
            self.name = name

        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"

    def scrap_cpu_stats(self, thefile, measurement_index, cpuidx, binary):
        """Read measurement data from file /proc/pid/stat
        See man proc
        """

        try:
            with open(thefile) as thestats:
                lines = thestats.readlines()
                sample = 0

                for line in lines:
                    entries = line.split()
                    if len(entries) == 52 and entries[1] == f"({binary})":
                        ut = int(entries[13])
                        st = int(entries[14])
                        ct = int(entries[15])
                        cs = int(entries[16])

                        self.insert_line(
                            idx=cpuidx,
                            mid=measurement_index,
                            sample=sample,
                            utime=ut,
                            stime=st,
                            cutime=ct,
                            cstime=cs,
                        )
                        sample += 1
                        cpuidx += 1

        except FileNotFoundError as e:
            logging.error("File not found, skipping for now!" + str(e))
            return cpuidx

        logging.debug(f"Read {sample} cpu stats")
        missing = self.data_length - sample
        for m in range(missing):
            self.insert_line(
                idx=cpuidx,
                mid=measurement_index,
                sample=sample,
                utime=0,
                stime=0,
                cutime=0,
                cstime=0,
            )
            sample += 1
            cpuidx += 1

        return cpuidx

    def postprocess(self, folders, testname, filename, binary):
        """Postprocess all relevant folders
        """
        cpuidx = 0
        for folder in folders:
            index = self.foldername_to_index(folder)

            statsfile = (
                f"{self.lake}/{folder}/PySys/{testname}/Output/linux/{filename}.out"
            )

            cpuidx = self.scrap_cpu_stats(statsfile, index, cpuidx, binary)

    def insert_line(self, idx, mid, sample, utime, stime, cutime, cstime):
        self.array[idx] = [idx, mid, sample, utime, stime, cutime, cstime]

    def show(self):
        """Show content with matplotlib
        """
        import matplotlib.pyplot as plt

        fig, ax = plt.subplots()

        # ax.plot(self.array[:,0], 'o-')
        ax.plot(self.array[:, 1], ".", label="mid")
        ax.plot(self.array[:, 2], "-", label="sample")
        ax.plot(self.array[:, 3], "-", label="utime")
        ax.plot(self.array[:, 4], "-", label="stime")
        ax.plot(self.array[:, 5], "-", label="cutime")
        ax.plot(self.array[:, 6], "-", label="cstime")
        plt.legend()
        plt.title("CPU History  " + self.name)

        plt.show()

    def update_table(self):
        """Create table and prepare loading via json and upload
        """
        logging.info("Updating table:" + self.name)
        self.delete_table()

        self.job_config = bigquery.LoadJobConfig(
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

        self.json_data = []

        for i in range(self.size):
            self.json_data.append(
                {
                    "id": int(self.array[i, 0]),
                    "mid": int(self.array[i, 1]),
                    "sample": int(self.array[i, 2]),
                    "utime": int(self.array[i, 3]),
                    "stime": int(self.array[i, 4]),
                    "cutime": int(self.array[i, 5]),
                    "cstime": int(self.array[i, 6]),
                }
            )

        self.upload_table()


class CpuHistoryStacked(MeasurementBase):
    """Class to represent a table of measured CPU usage as stacked graph.
    The graph contains user and system cpu time for the last N test-runs.
    """

    def __init__(self, size, client, testmode):
        self.size = size
        if testmode:
            self.name = "ci_cpu_hist_test"
        else:
            self.name = "ci_cpu_hist"
        self.client = client
        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"
        self.history = 10 # process the last 10 test runs
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
            ("t9s", "INT64"),
        ]
        self.array = np.zeros((size, len(self.fields)), dtype=np.int32)

    def postprocess(self,
        measurement_folders,
        data_length,
        cpu_array,
    ):
        """Postprocess all relevant folders
        """
        mlen = len(measurement_folders)

        # Set the id in the first column
        for i in range(data_length):
            self.array[i, 0] = i

        processing_range = min(len(measurement_folders), self.history)
        column = 1

        # Iterate backwards through the measurement list
        for m in range(mlen - 1, mlen - processing_range - 1, -1):

            for i in range(data_length):

                # Read user time from the cpu_table
                self.array[i, column] = cpu_array.array[m * data_length + i, 3]

                # Read system time from the cpu_table
                self.array[i, column + 1] = cpu_array.array[
                    m * data_length + i, 4
                ]

            column += 2

    def insert_line(self, line, idx):
        assert len(line) == len(self.fields)
        self.array[idx] = line

    def show(self):
        """Show content with matplotlib
        """
        import matplotlib.pyplot as plt

        fig, ax = plt.subplots()

        for i in range(len(self.fields)):
            if i % 2 == 0:
                style = "-o"
            else:
                style = "-x"
            ax.plot(self.array[:, i], style, label=self.fields[i][0])

        plt.legend()
        plt.title("CPU History Stacked  " + self.name)

        plt.show()

    def update_table(self):
        """Create table and prepare loading via json and upload
        """
        logging.info("Updating table:" + self.name)
        self.delete_table()

        schema = []
        for i in range(len(self.fields)):
            schema.append(bigquery.SchemaField(self.fields[i][0], self.fields[i][1]))

        self.job_config = bigquery.LoadJobConfig(schema=schema)

        self.json_data = []

        for i in range(self.size):
            line = {}
            for j in range(len(self.fields)):
                line[self.fields[j][0]] = int(self.array[i, j])
            self.json_data.append(line)

        self.upload_table()

class MemoryHistory(MeasurementBase):

    def __init__(self, lake, size, data_length, client, testmode):
        self.lake = lake
        self.data_length = data_length
        self.array = np.zeros((size, 8), dtype=np.int32)
        self.size = size
        self.client = client

        if testmode:
            self.name = "ci_mem_measurement_tedge_mapper_test"
        else:
            self.name = "ci_mem_measurement_tedge_mapper"

        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"


    def scrap_mem(self, thefile, mesaurement_index, memidx, arr):
        """Read measurement data from file
        """

        with open(thefile) as thestats:
            lines = thestats.readlines()
            sample = 0
            for line in lines:
                entries = line.split()
                size = entries[1 - 1]  #     (1) total program size
                resident = entries[2 - 1]  #   (2) resident set size
                shared = entries[3 - 1]  #     (3) number of resident shared pages
                text = entries[4 - 1]  #       (4) text (code)
                # lib = entries[5-1] #      (5) library (unused since Linux 2.6; always 0)
                data = entries[6 - 1]  #      (6) data + stack

                arr.insert_line(
                    idx=memidx,
                    mid=mesaurement_index,
                    sample=sample,
                    size=size,
                    resident=resident,
                    shared=shared,
                    text=text,
                    data=data,
                )
                sample += 1
                memidx += 1

        logging.debug(f"Read {sample} Memory stats")
        missing = self.data_length - sample
        for m in range(missing):

            arr.insert_line(
                idx=memidx,
                mid=mesaurement_index,
                sample=sample,
                size=0,
                resident=0,
                shared=0,
                text=0,
                data=0,
            )
            sample += 1
            memidx += 1

        return memidx

    def postprocess(self, folders, testname, filename, binary):
        """Postprocess all relevant folders
        """
        idx = 0
        for folder in folders:
            index = self.foldername_to_index(folder)

            statsfile = f"{self.lake}/{folder}/PySys/{testname}/Output/linux/{filename}.out"
            idx = self.scrap_mem( statsfile, index, idx, self)

    def insert_line(self, idx, mid, sample, size, resident, shared, text, data):
        self.array[idx] = [idx, mid, sample, size, resident, shared, text, data]

    def show(self):
        """Show content with matplotlib
        """
        import matplotlib.pyplot as plt

        fig, ax = plt.subplots()
        style = "."
        # ax.plot(self.array[:,0], 'o-')
        ax.plot(self.array[:, 1], style, label="mid")
        ax.plot(self.array[:, 2], style, label="sample")
        ax.plot(self.array[:, 3], style, label="size")
        ax.plot(self.array[:, 4], style, label="resident")
        ax.plot(self.array[:, 5], style, label="shared")
        ax.plot(self.array[:, 6], style, label="text")
        ax.plot(self.array[:, 7], style, label="data")

        plt.legend()
        plt.title("Memory History  " + self.name)

        plt.show()

    #    def update_table_one_by_one(self, dbo):
    #        for i in range(self.size):
    #            assert self.array[i, 0] == i
    #            q = (
    #                f"insert into {dbo}.{mem_table} values ( {i}, {self.array[i,1]},"
    #                f" {self.array[i,2]}, {self.array[i,3]},{self.array[i,4]},"
    #                f"{self.array[i,5]},{self.array[i,6]}, {self.array[i,7]} );"
    #            )
    #            # print(q)
    #            myquery(self.client, q)

    def update_table(self):
        """Create table and prepare loading via json and upload
        """
        logging.info("Updating table:" + self.name)
        self.job_config = bigquery.LoadJobConfig(
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

        self.json_data = []

        for i in range(self.size):
            self.json_data.append(
                {
                    "id": int(self.array[i, 0]),
                    "mid": int(self.array[i, 1]),
                    "sample": int(self.array[i, 2]),
                    "size": int(self.array[i, 3]),
                    "resident": int(self.array[i, 4]),
                    "shared": int(self.array[i, 5]),
                    "text": int(self.array[i, 6]),
                    "data": int(self.array[i, 6]),
                }
            )

        self.upload_table()
