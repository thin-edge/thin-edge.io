import os
from datetime import datetime, timedelta
from random import randint, shuffle
from typing import Optional

# this test will look at the date of current files in /var/log/tedge/agent/
# and create example files with the same date.


def get_file_timestamp() -> str:
        return (datetime.now() - timedelta(days=5, hours=6 - randint(0, 24))).strftime("%Y-%m-%dT%H:%M:%SZ")

def read_example_log():
    if not os.path.isdir("/var/log/tedge/agent"):
        return None

    content = ""
    for filename in os.listdir("/var/log/tedge/agent/"):
        if "software-list" in filename:
            content += open(f"/var/log/tedge/agent/{filename}", "r").read()
    return content.splitlines()

ERROR_MESSAGES = [
    f"Error: in line {randint(1, 10000)}.",
    "Error: No such file or directory: /home/some/file",
    "Error: Connection timed out. OS error 111.",
    "Error: Is MQTT running?",
    "Error: missing dependency mosquitto-clients."
]

def create_fake_logs(bad_lines_ratio=.3, num_lines=100) -> str:
    error_lines_no = int(bad_lines_ratio * num_lines)
    output = list()
    for _ in range(error_lines_no):
        output.append(ERROR_MESSAGES[randint(0, len(ERROR_MESSAGES) - 1)])

    log = read_example_log()
    if log is None:
        raise Exception("No log file found.")
    for _ in range(num_lines - error_lines_no):
        output.append(log[randint(0, len(log) - 1)])

    shuffle(output)
    return "\n".join(output)


if __name__ == "__main__":    
    file_names = ["example-log1", "example-log2", "example-log3"]
    file_sizes = [50, 100, 250]
    for idx, file_name in enumerate(file_names):
        timestamp = get_file_timestamp()
        with open(f"/var/log/tedge/agent/{file_name}-{timestamp}.log", "w") as handle:
            fake_log = create_fake_logs(num_lines=file_sizes[idx])
            handle.write(fake_log)


