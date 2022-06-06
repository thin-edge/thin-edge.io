import sys
import time
import os
import subprocess
import requests
from retry import retry
from test_log_generator import create_example_logs

"""
Validate end to end behaviour for the log request operation.

When we send a log request (for Error text with, 25 lines) from cumulocity to device and wait for some time.
Then sm mapper receives the request and sends the response
Validate if the response contains the log file for number of lines.
If number of lines are 50 Error messages then pass the test
Else stop and cleanup the operation by sending operation failed message.
"""

from environment_c8y import EnvironmentC8y


class LogRequestVerifySearchTextError(EnvironmentC8y):
    operation_id = None
    systemctl = "/usr/bin/systemctl"

    def setup(self):
        super().setup()
        self.create_logs_for_test()
        # Start c8y logfile request service
        log_file_daemon = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "c8y-log-plugin.service"],
            stdouterr="log_file_daemon",
        )

        self.addCleanupFunction(self.cleanup_logs)

    def execute(self):
        log_file_request_payload = {
            "dateFrom": "2022-06-04T12:55:49+0530",
            "dateTo": "2022-06-06T13:55:49+0530",
            "logFile": "software-management",
            "searchText": "Error",
            "maximumLines": 25,
        }
        self.operation_id = self.cumulocity.trigger_log_request(
            log_file_request_payload, self.project.deviceid
        )

    def validate(self):
        self.assertThat("True == value", value=self.wait_until_logs_retrieved())

    @retry(Exception, tries=20, delay=1)
    def wait_until_logs_retrieved(self):

        log_file = self.cumulocity.retrieve_log_file(self.operation_id)
        if len(log_file) != 0:
            return self.download_file_and_verify_error_messages(log_file)
        else:
            raise Exception("retry")

    def create_logs_for_test(self):
        # remove if there are any old files
        rm_logs = self.startProcess(
            command=self.sudo,
            arguments=["rm", "-rf", "/tmp/sw_logs"],
            stdouterr="rm_logs",
        )

        # create example logs
        create_example_logs()

        # move the logs
        move_logs = self.startProcess(
            command=self.sudo,
            arguments=["sh", "-c", "mv /tmp/sw_logs/* /var/log/tedge/agent/"],
            stdouterr="move_logs",
        )
       
    def download_file_and_verify_error_messages(self, url):
        get_response = requests.get(
            url, auth=(self.project.c8yusername, self.project.c8ypass), stream=False
        )
        nlines = get_response.content.decode("utf-8")
        return (
            sum([1 if line.startswith("Error") else 0 for line in nlines.split("\n")])
            == 25
        )

    def cleanup_logs(self):

        rm_logs = self.startProcess(
            command=self.sudo,
            arguments=["sh", "-c", "rm -rf /var/log/tedge/agent/software-log*"],
            stdouterr="rm_logs",
        )

        rm_tmp_logs = self.startProcess(
            command=self.sudo,
            arguments=["rm", "-rf", "/tmp/sw_logs"],
            stdouterr="rm_tmp_logs",
        )

        if self.getOutcome().isFailure():
            log = self.startProcess(
                command=self.sudo,
                arguments=[
                    self.tedge,
                    "mqtt",
                    "pub",
                    "c8y/s/us",
                    "502,c8y_LogfileRequest",
                ],
                stdouterr="send_failed",
            )

        log_file_daemon = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "c8y-log-plugin.service"],
            stdouterr="log_file_daemon",
        )
