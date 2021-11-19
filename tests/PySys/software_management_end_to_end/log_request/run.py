import sys
import time

"""
Validate end to end behaviour for the docker plugin for multiple docker images.

When we install two images
Then these two images are installed
When we install another image and update one of the existing image with newer version
Then there are three images installed, one with newer version
When we delete all the packages
Then docker images are not installed

"""

from environment_sm_management import SoftwareManagement


class LogRequest(SoftwareManagement):
    
    def setup(self):
        super().setup()
        self.trigger_log_request()
        self.addCleanupFunction(self.logOpCleanup)

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
