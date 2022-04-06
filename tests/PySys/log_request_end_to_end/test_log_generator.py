import os
from datetime import datetime, timedelta
from random import randint, shuffle, seed
from typing import Optional
from retry import retry

# this test will look at the date of current files in /var/log/tedge/agent/
# and create example files with the same date.


ERROR_MESSAGES = [
    "Error: in line 1000.",
    "Error: No such file or directory: /home/some/file",
    "Error: Connection timed out. OS error 111.",
    "Error: Is MQTT running?",
    "Error: missing dependency mosquitto-clients.",
    "thunderbird-gnome-support       1:78.14.0+build1-0ubuntu0.20.04.2",
    "thunderbird-locale-en-us        1:78.14.0+build1-0ubuntu0.20.04.2",
    "fonts-kacst-one 5.0+svn11846-10",
    "fonts-khmeros-core      5.0-7ubuntu1",
    "fonts-lao       0.0.20060226-9ubuntu1",
]


def create_fake_logs(num_lines=100) -> str:
    num_loops = int(num_lines / 10)
    output = "\n"
    for _ in range(num_loops):
        output += "\n".join(map(str, ERROR_MESSAGES))
        output += "\n"

    return output


class FailedToCreateLogs(Exception):
    pass


@retry(FailedToCreateLogs, tries=20, delay=1)
def check_files_created():
    if len(os.listdir("/tmp/sw_logs")) == 3:
        return True
    else:
        raise FailedToCreateLogs


def create_example_logs():
    file_names = ["example-log1", "example-log2", "example-log3"]
    file_sizes = [50, 100, 250]
    time_stamps = [
        "2021-11-18T13:15:10Z",
        "2021-11-19T21:15:10Z",
        "2021-11-20T13:15:10Z",
    ]
    os.mkdir("/tmp/sw_logs")
    for idx, file_name in enumerate(file_names):
        with open(f"/tmp/sw_logs/{file_name}-{time_stamps[idx]}.log", "w") as handle:
            fake_log = create_fake_logs(num_lines=file_sizes[idx])
            handle.write(fake_log)
    check_files_created()
