import sys
import time
import os
import subprocess
import requests
from retry import retry
"""
Validate end to end behaviour for the log request operation.

When we send a log request ( for all text with, 300 lines) from cumulocity to device and wait for some time.
Then sm mapper receives the request and sends the response
Validate if the response contains the log file for number of lines.
If number of lines are greater than 300 then pass the test
Else stop and cleanup the operation by sending operation failed message.
"""

from environment_c8y import EnvironmentC8y


class LogRequestVerifyNumberOfLines(EnvironmentC8y):
    operation_id = None

    def setup(self):
        super().setup()
        self.create_logs_for_test()
        self.addCleanupFunction(self.cleanup_logs)

    def execute(self):
        log_file_request_payload = {
            "dateFrom": "2021-11-15T18:55:49+0530",
            "dateTo": "2021-11-21T18:55:49+0530",
            "logFile": "software-management",
            "searchText": "",
            "maximumLines": 300
        }

        self.operation_id = self.cumulocity.trigger_log_request(
            log_file_request_payload, self.project.deviceid)

    def validate(self):
        self.assertThat("True == value",
                        value=self.wait_until_logs_retrieved())

    @retry(Exception, tries=20, delay=1)
    def wait_until_logs_retrieved(self):
        log_file = self.cumulocity.retrieve_log_file(self.operation_id)
        if len(log_file) != 0:
            return self.download_file_and_verify_number_of_lines(log_file)
        else:
            raise Exception("retry")

    def create_logs_for_test(self):
        log = self.startProcess(
            command=self.sudo,
            arguments=[
                "python3", f"{os.getcwd()}/log_request_end_to_end/create_test_logs.py"],
            stdouterr="log_failed",
        )

    def download_file_and_verify_number_of_lines(self, url):
        get_response = requests.get(url, auth=(
            self.project.username, self.project.c8ypass), stream=False)
        
        nlines = len(get_response.content.decode('utf-8').split('\n')[:-1])
        # The log lines are concatenated from 3 different log files, so there will be 3 extra lines.
        if nlines == 303:
            return True
        else:
            return False

    def cleanup_logs(self):
        # Removing files form startProcess is not working
        os.system("sudo rm -rf /var/log/tedge/agent/example-*")
        if self.getOutcome().isFailure():
            log = self.startProcess(
                command=self.sudo,
                arguments=[self.tedge, "mqtt", "pub",
                           "c8y/s/us", "502,c8y_LogfileRequest"],
                stdouterr="send_failed",
            )
