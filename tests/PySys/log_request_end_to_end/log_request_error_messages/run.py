import sys
import time
import os
import subprocess
import requests

"""
Validate end to end behaviour for the log request operation.

When we send a log request (for Error text with, 50 lines) from cumulocity to device and wait for some time.
Then sm mapper receives the request and sends the response
Validate if the response contains the log file for number of lines.
If number of lines are 50 Error messages then pass the test
Else stop and cleanup the operation by sending operation failed message.
"""

from environment_c8y import EnvironmentC8y


class LogRequestVerifyErrorMesages(EnvironmentC8y):

    def setup(self):
        super().setup()
        self.create_logs_for_test()
        self.addCleanupFunction(self.cleanup_logs)

    def execute(self):
        log_file_request_payload = {
            "dateFrom": "2021-11-15T18:55:49+0530",
            "dateTo": "2021-11-21T18:55:49+0530",
            "logFile": "software-management",
            "searchText": "Error",
            "maximumLines": 50
        }
        self.cumulocity.trigger_log_request(log_file_request_payload)

    def validate(self):
        status = self.wait_until_logs_retrieved()
        if not status:
            self.log.info("Explicitly send the operation status as failed.")
            self.stopLogRequestOp()
        else:
            self.assertThat("True == value", value=status)

    def wait_until_logs_retrieved(self):
        for i in range(1, 20):
            time.sleep(1)
            log_file = self.cumulocity.retrieve_log_file()
            if len(log_file) != 0:
                if self.download_file_and_verify_error_messages(log_file):
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
                "python3", f"{os.getcwd()}/log_request_end_to_end/create_test_logs.py"],
            stdouterr="log_failed",
        )

    def download_file_and_verify_error_messages(self, url):
        get_response = requests.get(url, auth=(
            self.project.username, self.project.c8ypass), stream=True)
        nErrors = 0
        for chunk in get_response.iter_content(chunk_size=1024):
              nErrors += chunk.decode('utf-8').count("Error")
        if nErrors >= 49:
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
