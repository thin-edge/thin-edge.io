import sys
import time
import os
from datetime import datetime, timedelta
from random import randint, shuffle
from typing import Optional
import subprocess
import shlex

"""
Validate end to end behaviour for the log request operation.

When we send a log request from cumulocity to device and wait for some time.
Then sm mapper receives the request and sends the response
Validate if the response contains the log file or not.
If there is log file, exit the test successfully.
Else stop and cleanup the operation by sending operation failed message.
"""

from environment_sm_management import SoftwareManagement


class LogRequest(SoftwareManagement):
   
    def setup(self):        
        super().setup()
        self.create_logs_for_test()
        self.addCleanupFunction(self.cleanup_logs)
        
    def execute(self):
        log_file_request_payload = {
                "dateFrom":"2021-11-16T18:55:49+0530",
                "dateTo":"2021-11-18T18:55:49+0530",
                "logFile":"software-management",
                "searchText":"Error",
                "maximumLines":10
            }

        self.trigger_log_request(log_file_request_payload)

    def validate(self):
        status = self.wait_until_retrieved_logs()
        if not status:
            self.log.info("failed, explicitly failing request")
            self.stopLogOpCleanup()
        else:
           self.assertThat("True == value", value=status)

      
    def stopLogOpCleanup(self): 
        log = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "c8y/s/us/", "502,c8y_LogfileRequest"],
            stdouterr="send_failed",           
        )

    def wait_until_retrieved_logs(self):
        for i in range(1,20):
            time.sleep(1)
            if self.check_if_log_req_complete():
                return True
            else:
                continue
        return False

    def create_logs_for_test(self):
        log = self.startProcess(
            command=self.sudo,
            arguments=["python3", f"{os.getcwd()}/software_management_end_to_end/log_request/create_test_logs.py"],
            stdouterr="log_failed",           
        )

    
    def cleanup_logs(self):
        # Removing files form startProcess is not working
        os.system("sudo rm -rf /var/log/tedge/agent/*")
       