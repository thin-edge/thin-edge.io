import json
import logging
import os
import sys
import time
import numpy as np

from google.cloud import bigquery

logging.basicConfig(level=logging.INFO)

cpu_table = "ci_cpu_measurement_tedge_mapper"
mem_table = "ci_mem_measurement_tedge_mapper"
cpu_hist_table = "ci_cpu_hist"


def get_database(style: str):
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
        sys.exit(1)

    return client, dbo, integer, conn


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


def scrap_measurement_metadata(file):
    with open(file) as content:
        data = json.load(content)
        run = data["run_number"]
        date = data["updated_at"]
        url = data["html_url"]
        name = data["name"]
        branch = data["head_branch"]

    return run, date, url, name, branch


class MeasurementBase:
    def upload_table(self):

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


class MeasurementMetadata(MeasurementBase):
    def __init__(self, size, client, testmode, lake):
        self.array = []
        self.client = client
        self.size = size
        if testmode:
            self.name = "ci_measurements_test"
        else:
            self.name = "ci_measurements"
        self.lake = lake

        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"

    def postprocess(self, folders):
        i = 0
        for folder in folders:
            index = int(folder.split("_")[1].split(".")[0])

            # lake = os.path.expanduser("~/DataLakeTest")
            name = f"system_test_{index}_metadata.json"
            path = os.path.join(self.lake, name)

            run, date, url, name, branch = scrap_measurement_metadata(path)
            self.array.append((i, run, date, url, name, branch))
            i += 1

        return self.array

    def show(self):
        logging.info(f"Content of table {self.database}")
        for row in self.array:
            logging.info(row)

    def update_table(self):

        logging.info("Updating table:" + self.name)
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

        # print(self.array)
        j = 0
        for i in range(self.size):
            # print(self.size, i, j)
            self.json_data.append(
                {
                    "id": self.array[i][0],
                    "mid": self.array[i][1],
                    "date": self.array[i][2],
                    "url": self.array[i][3],
                    "name": self.array[i][4],
                    "branch": self.array[i][5],
                }
            )
            j += 1

        self.upload_table()

    def delete_table(self):
        try:
            self.client.delete_table(self.database)
        except:  # google.api_core.exceptions.NotFound:
            pass


def scrap_cpu(data_length, thefile, mesaurement_index, cpuidx, arr, binary):

    try:
        with open(thefile) as thestats:
            lines = thestats.readlines()
            sample = 0

            for line in lines:
                entries = line.split()
                if len(entries) == 52 and entries[1] == f"({binary})":
                    ut = int(entries[14 - 1])
                    st = int(entries[15 - 1])
                    ct = int(entries[16 - 1])
                    cs = int(entries[17 - 1])
                    # print(idx, ut,st,ct,cs)

                    arr.insert_line(
                        idx=cpuidx,
                        mid=mesaurement_index,
                        sample=sample,
                        utime=ut,
                        stime=st,
                        cutime=ct,
                        cstime=cs,
                    )
                    sample += 1
                    cpuidx += 1
    except FileNotFoundError as e:
        logging.error("File not found !!!" + str(e))
        return cpuidx

    logging.debug(f"Read {sample} cpu stats")
    missing = data_length - sample
    for m in range(missing):
        arr.insert_line(
            idx=cpuidx,
            mid=mesaurement_index,
            sample=sample,
            utime=0,
            stime=0,
            cutime=0,
            cstime=0,
        )
        sample += 1
        cpuidx += 1

    return cpuidx


class CpuHistory(MeasurementBase):
    """Mostly the representation of a unpublished SQL table"""

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
        return scrap_cpu(
            self.data_length, thefile, measurement_index, cpuidx, self, binary
        )

    def postprocess(self, folders, testname, filename, binary):
        cpuidx = 0
        for folder in folders:
            measurement_index = int(folder.split("_")[1].split(".")[0])

            statsfile = (
                f"{self.lake}/{folder}/PySys/{testname}/Output/linux/{filename}.out"
            )

            cpuidx = self.scrap_cpu_stats(statsfile, measurement_index, cpuidx, binary)

    def insert_line(self, idx, mid, sample, utime, stime, cutime, cstime):
        self.array[idx] = [idx, mid, sample, utime, stime, cutime, cstime]

    def show(self):
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

    def delete_table(self):
        try:
            self.client.delete_table(self.database)
        except:  # google.api_core.exceptions.NotFound:
            pass

    def update_table(self):
        logging.info("Updating table:" + self.name)
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
    """Mostly the representation of a unpublished SQL table"""

    def __init__(self, size, client, testmode):
        self.size = size
        if testmode:
            self.name = "ci_cpu_hist_test"
        else:
            self.name = "ci_cpu_hist"
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
        self.client = client
        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"

    def insert_line(self, line, idx):
        assert len(line) == len(self.fields)
        self.array[idx] = line

    def show(self):
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

    def delete_table(self):
        try:
            self.client.delete_table(f"sturdy-mechanic-312713.ADataSet.{self.name}")
        except:  # google.api_core.exceptions.NotFound:
            pass

    def update_table(self):
        logging.info("Updating table:" + self.name)
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
    def __init__(self, size, client, testmode):
        self.array = np.zeros((size, 8), dtype=np.int32)
        self.size = size
        self.client = client

        if testmode:
            self.name = "ci_mem_measurement_tedge_mapper_test"
        else:
            self.name = "ci_mem_measurement_tedge_mapper"

        self.database = f"sturdy-mechanic-312713.ADataSet.{self.name}"

    def insert_line(self, idx, mid, sample, size, resident, shared, text, data):
        self.array[idx] = [idx, mid, sample, size, resident, shared, text, data]

    def show(self):
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

    def delete_table(self):
        try:
            self.client.delete_table(self.database)
        except:  # google.api_core.exceptions.NotFound:
            pass

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
