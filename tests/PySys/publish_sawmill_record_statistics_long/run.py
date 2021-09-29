import os
from pathlib import Path
import platform
import sys
import rrdtool


sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Publish sawmill and record process statistics long version

Given a configured system with configured certificate
When we derive from EnvironmentC8y
When we publish with the sawtooth_publisher with 100ms cycle time and publish
    6 times 100 values to the Sawmill topic (10 on each publish) (60s).
When we record the output of mosquittos $SYS/# topic
When we record the /proc/pid/status of mosquitto
When we record the /proc/pid/status of tedge-mapper
When we upload the data to the github action for further analysis (not done here)

TODO : Add validation procedure
"""


class PublishSawmillRecordStatisticsLong(EnvironmentC8y):
    def setup(self):
        super().setup()
        self.log.info("Setup")
        self.addCleanupFunction(self.mycleanup)

    def execute(self):
        super().execute()
        self.log.info("Execute")

        collectd = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "start", "collectd"],
            stdouterr="collectd_out",
        )

        sub = self.startProcess(
            command="/usr/bin/mosquitto_sub",
            arguments=["-v", "-h", "localhost", "-t", "$SYS/#"],
            stdouterr="mosquitto_sub_stdout",
            background=True,
        )

        # start the publisher

        publisher = self.project.exampledir + "/sawtooth_publisher"
        cmd = os.path.expanduser(publisher)

        pub = self.startProcess(
            command=cmd,
            # run for one minute
            # 20ms wait time, height 100, 60 repetitions -> 120s
            arguments=["20", "100", "60", "sawmill"],
            stdouterr="stdout_sawmill",
        )

    def validate(self):
        super().validate()
        self.assertGrep('mosquitto_sub_stdout.out', 'mosquitto', contains=True)

    def mycleanup(self):

        self.log.info("My Cleanup")

        collectd = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "collectd"],
            stdouterr="collectd_out",
        )

        node = platform.node()

        path = Path(f'/var/lib/collectd/rrd/{node}.local/exec/')
        if not path.exists():
            # the hosted rpis don't have the ".local" so we need a workaround here
            path = Path(f'/var/lib/collectd/rrd/{node}/exec/')
            if not path.exists():
                raise SystemError("Cannot find derive place for collectd measurements")

        for a_file in path.iterdir():

            filename = str(a_file.resolve())
            self.log.info("Exporting data from %s"%filename)
            result = rrdtool.fetch(filename, "LAST")
            start, end, step = result[0]

            rows = result[2]
            values=rows[-120:]
            assert step==1
            counter = end-120

            with open( os.path.join(self.output, a_file.name+".txt"),"w") as myfile:
                for i in values:
                    myfile.write( f"{counter} {i[0]}\n")
                    counter += step

            assert counter == end