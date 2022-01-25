import subprocess
import time


from pysys.basetest import BaseTest

class TedgeEnvironment(BaseTest):

    def wait_if_restarting_mosquitto_too_fast(self):
        """Make sure we do not restart mosqiotto too fast
        Systemd will become suspicios when whe restart faster than 5 seconds
        """
        minimum_time = 15 #5
        etimes = subprocess.check_output("/usr/bin/ps -o etimes $(pidof mosquitto)", shell=True)
        runtime = int(etimes.split()[1])
        if runtime <= minimum_time:
            self.log.info(f"Restarting mosquitto too fast in the last {minimum_time} seconds. It was only up for {runtime} seconds" )
            # Derive additional delay time and add one safety second
            delay = minimum_time -runtime +1
            self.log.info(f"Delaying execution by {delay} seconds" )
            time.sleep( delay)

