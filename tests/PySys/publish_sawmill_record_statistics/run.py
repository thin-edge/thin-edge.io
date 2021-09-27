import os
import sys
import rrdtool

sys.path.append("environments")
from environment_c8y import EnvironmentC8y

"""
Publish sawmill and record process statistics

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


class PublishSawmillRecordStatistics(EnvironmentC8y):
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

        # record /proc/pid/status

        #status_mosquitto = self.startProcess(
        #    command="/bin/sh",
        #    arguments=[
        #        "-c",
        #        "while true; do date; cat /proc/$(pgrep -x mosquitto)/status; sleep 1; done",
        #    ],
        #    stdouterr="status_mosquitto_stdout",
        #    background=True,
        #)

        #status_mapper = self.startProcess(
        #    command="/bin/sh",
        #    arguments=[
        #        "-c",
        #        "while true; do date; cat /proc/$(pgrep -f -x \"/usr/bin/tedge_mapper c8y\")/status; sleep 1; done",
        #    ],
        #    stdouterr="status_mapper_stdout",
        #    background=True,
        #)

        # record /proc/pid/stat

        #stats_mapper = self.startProcess(
        #    command="/bin/sh",
        #    arguments=[
        #        "-c",
        #        "while true; do cat /proc/$(pgrep -f -x \"/usr/bin/tedge_mapper c8y\")/stat; sleep 1; done",
        #    ],
        #    stdouterr="stat_mapper_stdout",
        #    background=True,
        #)

        #stats_mosquitto = self.startProcess(
        #    command="/bin/sh",
        #    arguments=[
        #        "-c",
        #        "while true; do cat /proc/$(pgrep -x mosquitto)/stat; sleep 1; done",
        #    ],
        #    stdouterr="stat_mosquitto_stdout",
        #    background=True,
        #)

        # record /proc/pid/statm

        #statm_mapper = self.startProcess(
        #    command="/bin/sh",
        #    arguments=[
        #        "-c",statm_mapper = self.startProcess(
        #    command="/bin/sh",
        #    arguments=[
        #        "-c",
        #        "while true; do cat /proc/$(pgrep -f -x \"/usr/bin/tedge_mapper c8y\")/statm; sleep 1; done",
        #    ],
        #    stdouterr="statm_mapper_stdout",
        #    background=True,
        #)
        #statm_mosquitto = self.startProcess(
        #    command="/bin/sh",
        #    arguments=[
        #        "-c",
        #        "while true; do cat /proc/$(pgrep -x mosquitto)/statm; sleep 1; done",
        #    ],
        #    stdouterr="statm_mosquitto_stdout",
        #    background=True,
        #)

        # start the publisher

        publisher = self.project.exampledir + "/sawtooth_publisher"
        cmd = os.path.expanduser(publisher)

        pub = self.startProcess(
            command=cmd,
            # run for one minute
            arguments=["100", "100", "6", "sawmill"],
            stdouterr="stdout_sawmill",
        )

        super().validate()

        # These are mostly placeholder validations to make sure
        # that the file is there and is at least not empty
        self.assertGrep('mosquitto_sub_stdout.out', 'mosquitto', contains=True)

        #self.assertGrep('status_mapper_stdout.out', 'tedge_mapper', contains=True)
        #self.assertGrep('status_mosquitto_stdout.out', 'mosquitto', contains=True)

        #self.assertGrep('stat_mapper_stdout.out', 'tedge_mapper', contains=True)
        #self.assertGrep('stat_mosquitto_stdout.out', 'mosquitto', contains=True)

        # Match 7 numbers separated by space
        #self.assertGrep('statm_mapper_stdout.out', expr=r'(\d+ ){6}\d+', contains=True)
        #self.assertGrep('statm_mosquitto_stdout.out', expr=r'(\d+ ){6}\d+', contains=True)




    def mycleanup(self):

        collectd = self.startProcess(
            command=self.sudo,
            arguments=[self.systemctl, "stop", "collectd"],
            stdouterr="collectd_out",
        )

        self.log.info("My Cleanup")

        from pathlib import Path

        # ghraspberrypi.local
        #p = Path('/var/lib/collectd/rrd/isonoe.local/exec/')
        p = Path('/var/lib/collectd/rrd/ghraspberrypi.local/exec/')
        for x in p.iterdir():
            #print(x.resolve(), x.name)

            filename = str(x.resolve())
            self.log.info("Analysing %s"%filename)
            result = rrdtool.fetch(filename, "LAST")
            start, end, step = result[0]
            #self.log.info("Start %s, End %s, Step %s"%(start, end, step))

            ds = result[1]
            rows = result[2]
            values=rows[-60:]
            assert step==1
            counter = end-60

            with open("publish_sawmill_record_statistics/Output/linux/"+x.name+".txt","w") as myfile:
                for i in values:
                    #print(counter, i[0])
                    counter += step
                    myfile.write( f"{counter} {i[0]}\n")

            #self.log.info("counter %s end %s"%(counter, end))
            assert counter == end
