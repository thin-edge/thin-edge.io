import sys
import time
import os
from datetime import datetime, timedelta
from random import randint, shuffle
from typing import Optional
import subprocess
import shlex
import requests

"""
Validate end to end behaviour for the log request operation.

When we send a log request ( for all text with, 25 lines) from cumulocity to device and wait for some time.
Then sm mapper receives the request and sends the response
Validate if the response contains the log file for number of lines.
If number of lines are greater than 25 then pass the test
Else stop and cleanup the operation by sending operation failed message.
"""

from environment_sm_management import SoftwareManagement


class LogRequestVerifyNumberOfLines(SoftwareManagement):

    def setup(self):
        super().setup()
        self.create_logs_for_test()
        self.addCleanupFunction(self.cleanup_logs)

    def execute(self):
        log_file_request_payload = {
            "dateFrom": "2021-11-16T18:55:49+0530",
            "dateTo": "2021-11-18T18:55:49+0530",
            "logFile": "software-management",
            "searchText": "",
            "maximumLines": 25
        }
        self.trigger_log_request(log_file_request_payload)

    def validate(self):
        status = self.wait_until_retrieved_logs()
        if not status:
            self.log.info("Explicitly send the operation status as failed.")
            self.stopLogRequestOp()
        else:
            self.assertThat("True == value", value=status)

    def wait_until_retrieved_logs(self):
        for i in range(1, 20):
            time.sleep(1)
            log_file = self.check_if_log_req_complete()
            if len(log_file) != 0:
                if self.download_file_and_verify_number_of_lines(log_file):
                    return True
                else:
                    return False
            else:
                continue
        return False

    def create_logs_for_test(self):
        log = self.startProcess(
            command=self.sudo,
            arguments=[
                "python3", f"{os.getcwd()}/software_management_end_to_end/log_request/create_test_logs.py"],
            stdouterr="log_failed",
        )
        # wait for logs to get created
        time.sleep(1)

    def download_file_and_verify_number_of_lines(self, url):
        get_response = requests.get(url, auth=(
            self.project.username, self.project.c8ypass), stream=True)
        nlines = 0
        for chunk in get_response.iter_content(chunk_size=1024):
            nlines += len(chunk.decode('utf-8').split('\n'))

        self.log.info("num logs %d", nlines)
        if nlines > 25:
            return True
        else:
            return False

    def stopLogRequestOp(self):
        log = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub",
                       "c8y/s/us/", "502,c8y_LogfileRequest"],
            stdouterr="send_failed",
        )

    def cleanup_logs(self):
        # Removing files form startProcess is not working
        os.system("sudo rm -rf /var/log/tedge/agent/example-*")
