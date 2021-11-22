import sys
import time

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
        self.addCleanupFunction(self.logOpCleanup)

    def execute(self):

        log_file_request_payload = {
                "dateFrom":"2021-11-17T18:55:49+0530",
                "dateTo":"2021-11-19T18:55:49+0530",
                "logFile":"software-management",
                "searchText":"Error",
                "maximumLines":1000
            }

        self.trigger_log_request(log_file_request_payload)

    def validate(self):
        status = self.wait_until_installed()
        if not status:
            self.log.info("failed, explicitly failing request")
        else:
           self.assertThat("True == value", value=status)

      
    def logOpCleanup(self): 
        pub = self.startProcess(
            command=self.sudo,
            arguments=[self.tedge, "mqtt", "pub", "c8y/s/us/", "502,c8y_LogfileRequest"],
            stdouterr="send_failed",           
        )

    def wait_until_installed(self):
        for i in range(1,20):
            time.sleep(1)
            if self.check_if_log_req_complete():
                return True
            else:
                continue
        return False
